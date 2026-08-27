//! Rendering: project-grouped sidebar (collapsible headers, pinned section,
//! drag-resizable width) + live preview pane + status/confirm bar.

use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use unpeel_core::terminal_viewport::{TerminalViewportRow, TerminalViewportSnapshot};

use crate::sessions::{SidebarItem, Status};
use crate::snapshots::SnapshotService;
use crate::{App, Modal};

pub const MIN_SIDEBAR_WIDTH: u16 = 20;
pub const MAX_SIDEBAR_WIDTH: u16 = 100;
/// The clickable label on the sidebar's bottom edge that opens the footer
/// menu. Plain ASCII on purpose: a decorative glyph like ☰ has ambiguous
/// East-Asian width, so terminals that render it two cells wide (Ghostty does)
/// shift the whole bottom border by a cell and move the label out from under
/// the mouse hit-test.
pub const MENU_LABEL: &str = " menu ";

/// Whether a click on the sidebar's bottom border row lands on the menu label.
/// Deliberately generous — the corner cell through a cell past the label — so
/// a small width difference in the label never makes the click miss.
pub fn menu_label_hit(col: u16) -> bool {
    col <= MENU_LABEL.chars().count() as u16 + 1
}

/// The fold-all toggle on the sidebar's bottom-right — the mouse peer of the
/// `-` key: one click folds every project, and once everything is folded the
/// label flips to "+" to expand again. Plain ASCII for the same reason as
/// `MENU_LABEL`.
pub const FOLD_LABEL: &str = " - ";
pub const UNFOLD_LABEL: &str = " + ";

/// Hover action at the right edge of project and child-folder rows.
/// Kept in one place so the painted label and mouse hit target stay aligned.
pub const HEADER_ADD_LABEL: &str = "+ New";

/// Whether a click on the sidebar's bottom border row lands on the fold-all
/// toggle. Generous like `menu_label_hit`: a cell before the label through
/// the corner. Callers already exclude the divider column itself (that cell
/// starts a width drag).
pub fn fold_label_hit(col: u16, divider_col: u16) -> bool {
    col + FOLD_LABEL.chars().count() as u16 + 1 >= divider_col
}

/// The activity control is a compact right-aligned title on the sidebar's
/// top border. Its painted face is 3–4 cells (spinner/bell plus optional
/// unread dot); a five-cell target keeps it comfortable without eating the
/// Projects title at the minimum sidebar width.
pub fn activity_button_hit(col: u16, divider_col: u16) -> bool {
    col < divider_col && col.saturating_add(5) >= divider_col
}

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A highlight that TRAVELS across the label (skeleton-loader style)
/// rather than pulsing its brightness — pulsing the whole word reads as
/// blinking. Each character is lit by its distance from a band sweeping
/// left to right.
fn shimmer_spans(text: &str, base: Color) -> Vec<Span<'static>> {
    let Color::Rgb(r, g, b) = base else {
        return vec![Span::styled(
            text.to_string(),
            Style::default().fg(base).add_modifier(Modifier::BOLD),
        )];
    };
    let chars: Vec<char> = text.chars().collect();
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    const PERIOD_MS: u128 = 1_800;
    let width = chars.len().max(1) as f32;
    // Sweep a little past both ends so the band enters and leaves cleanly.
    let travel = width + 6.0;
    let head = (millis % PERIOD_MS) as f32 / PERIOD_MS as f32 * travel - 3.0;
    chars
        .into_iter()
        .enumerate()
        .map(|(i, ch)| {
            let distance = (i as f32 - head).abs();
            let lift = (1.0 - (distance / 3.0)).max(0.0) * 0.45;
            let scale = 1.0 + lift;
            let clamp = |v: u8| ((v as f32 * scale).clamp(0.0, 255.0)) as u8;
            Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(Color::Rgb(clamp(r), clamp(g), clamp(b)))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

fn spinner_frame() -> &'static str {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    SPINNER_FRAMES[(millis / 100) as usize % SPINNER_FRAMES.len()]
}

/// Brand-ish accent per CLI, mirroring the desktop's per-provider spinner
/// tinting.
pub fn cli_color(command: &str) -> Color {
    crate::runtime_presentation::tint_rgb(command)
        .map(|(red, green, blue)| Color::Rgb(red, green, blue))
        .unwrap_or(Color::Yellow)
}

/// Status marker: busy/starting sessions animate a braille spinner in the
/// CLI's accent color; settled states keep their static glyphs.
fn status_span_with_unread(status: Status, command: &str, unread: bool) -> Span<'static> {
    match status {
        Status::Busy | Status::Starting => Span::styled(
            spinner_frame().to_string(),
            Style::default().fg(cli_color(command)),
        ),
        Status::Attention => Span::styled(
            Status::Attention.glyph().to_string(),
            Style::default().fg(status_color(Status::Attention)),
        ),
        // Settled states carry no marker — unless the session settled while
        // unobserved, which earns the desktop's blue unread dot.
        Status::Idle | Status::Exited if unread => {
            Span::styled("●", Style::default().fg(Color::Rgb(64, 140, 255)))
        }
        Status::Idle | Status::Exited => Span::raw(" "),
    }
}

fn git_branch_of(cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    let head = std::fs::read_to_string(std::path::Path::new(cwd).join(".git/HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(branch) => branch.to_string(),
        None => head.chars().take(7).collect(),
    })
}

/// Whether the hosting terminal has a light background — probed once at
/// startup (OSC 11, then COLORFGBG; dark when neither answers) so the
/// selection can pick its pole. See `detect_light_background` in main.
static LIGHT_BACKGROUND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_light_background(light: bool) {
    LIGHT_BACKGROUND.store(light, std::sync::atomic::Ordering::Relaxed);
}

fn light_background() -> bool {
    LIGHT_BACKGROUND.load(std::sync::atomic::Ordering::Relaxed)
}

/// Muted-but-legible secondary text. `DarkGray` maps to the theme's
/// brightest-black, which on Ghostty's default is a hair above the
/// background — fine for hairlines, unreadable for words.
pub const MUTED: Color = Color::Rgb(150, 156, 170);

/// Project headers: structure, so a shade under body text but nowhere near
/// the near-invisible grey they started as.
pub const HEADER: Color = Color::Rgb(196, 202, 216);

/// The focused-session frame. An explicit RGB rather than palette `Cyan`,
/// which most terminal themes (Ghostty's default included) render as a
/// green-teal — this reads as the intended purple-gray everywhere.
pub const FOCUS: Color = Color::Rgb(156, 147, 184);

/// Attention markers mirror the desktop app's `Theme.attention` token.
pub const ATTENTION: Color = Color::Rgb(245, 158, 11);

/// Compact action/keycap badge used by inline folder actions and settings
/// hints. The muted fill gives the control a shape without competing with
/// status or selection color.
fn folder_badge(label: impl Into<String>) -> Span<'static> {
    let (bg, fg) = if light_background() {
        (Color::Rgb(222, 226, 233), Color::Rgb(58, 62, 72))
    } else {
        (Color::Rgb(44, 48, 56), Color::Rgb(206, 211, 219))
    };
    Span::styled(
        format!(" {} ", label.into()),
        Style::default().bg(bg).fg(fg),
    )
}

/// A badge that sits against a right edge: keep its leading breathing room,
/// but drop the final padding cell so the label itself ends at the edge.
fn right_flush_folder_badge(label: impl Into<String>) -> Span<'static> {
    let mut badge = folder_badge(label);
    badge.content = badge.content.trim_end().to_owned().into();
    badge
}

/// Child-folder session count, presented like session dates: quiet ink with
/// no badge fill. Parentheses keep it distinct from the age column.
fn folder_count(count: usize) -> Span<'static> {
    Span::styled(format!("({count})"), Style::default().fg(Color::DarkGray))
}

pub fn status_color(status: Status) -> Color {
    match status {
        Status::Starting => Color::Cyan,
        Status::Busy => Color::Yellow,
        Status::Idle => Color::Green,
        Status::Attention => ATTENTION,
        Status::Exited => Color::DarkGray,
    }
}

fn parse_color(spec: &str) -> Option<Color> {
    if let Some(n) = spec.strip_prefix("ansi:") {
        return n.parse::<u8>().ok().map(Color::Indexed);
    }
    if let Some(n) = spec.strip_prefix("ansi256:") {
        return n.parse::<u8>().ok().map(Color::Indexed);
    }
    if let Some(rgb) = spec.strip_prefix("rgb:") {
        let mut parts = rgb.splitn(3, ',');
        let r = parts.next()?.parse::<u8>().ok()?;
        let g = parts.next()?.parse::<u8>().ok()?;
        let b = parts.next()?.parse::<u8>().ok()?;
        return Some(Color::Rgb(r, g, b));
    }
    None
}

/// Snapshot rows arrive with trailing blanks trimmed, while their style
/// runs still describe the full grid width — so a background that runs to
/// the edge (opencode's themed panes, any full-width fill) would otherwise
/// stop at the last printable character and leave the terminal's own
/// background showing through. Pad to `cols` and apply the runs across the
/// padded row so those cells keep their color.
fn row_to_line(
    row: &TerminalViewportRow,
    cols: u16,
    selection: Option<(u16, u16)>,
) -> Line<'static> {
    let text_width: usize = row.text.graphemes(true).map(UnicodeWidthStr::width).sum();
    let width = (cols as usize).max(text_width);
    let mut styles: Vec<Style> = vec![Style::default(); width];
    for run in &row.styles {
        let start = run.start as usize;
        if start >= styles.len() {
            continue;
        }
        let end = (start + run.len as usize).min(styles.len());
        let mut style = Style::default();
        if let Some(fg) = run.fg.as_deref().and_then(parse_color) {
            style = style.fg(fg);
        }
        if let Some(bg) = run.bg.as_deref().and_then(parse_color) {
            style = style.bg(bg);
        }
        if run.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if run.inverse {
            style = style.add_modifier(Modifier::REVERSED);
        }
        for slot in &mut styles[start..end] {
            *slot = style;
        }
    }
    let mut styled_graphemes = Vec::with_capacity(row.text.graphemes(true).count() + width);
    let mut cell_col = 0usize;
    for grapheme in row.text.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        let style_col = if grapheme_width == 0 {
            cell_col.saturating_sub(1)
        } else {
            cell_col
        };
        let mut style = styles.get(style_col).copied().unwrap_or_default();
        let next_col = cell_col.saturating_add(grapheme_width);
        if selection.is_some_and(|(start, end)| {
            let glyph_start = if grapheme_width == 0 {
                style_col
            } else {
                cell_col
            };
            let glyph_end = if grapheme_width == 0 {
                glyph_start + 1
            } else {
                next_col
            };
            glyph_end > start as usize && glyph_start <= end as usize
        }) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        styled_graphemes.push((grapheme, style));
        cell_col = next_col;
    }
    while cell_col < width {
        let mut style = styles[cell_col];
        if selection
            .is_some_and(|(start, end)| cell_col >= start as usize && cell_col <= end as usize)
        {
            style = style.add_modifier(Modifier::REVERSED);
        }
        styled_graphemes.push((" ", style));
        cell_col += 1;
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut buffer = String::new();
    let mut current = Style::default();
    for (grapheme, style) in styled_graphemes {
        if style != current && !buffer.is_empty() {
            spans.push(Span::styled(std::mem::take(&mut buffer), current));
        }
        current = style;
        buffer.push_str(grapheme);
    }
    if !buffer.is_empty() {
        spans.push(Span::styled(buffer, current));
    }
    Line::from(spans)
}

fn snapshot_lines(
    snapshot: &TerminalViewportSnapshot,
    selection: Option<&crate::TerminalSelection>,
) -> Vec<Line<'static>> {
    snapshot
        .viewport_rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            row_to_line(
                row,
                snapshot.cols,
                selection.and_then(|value| value.row_range(index)),
            )
        })
        .collect()
}

/// Exact on-screen rectangle occupied by the child terminal grid. The
/// preview can letterbox a phone-sized/narrow grid, so mouse hit-testing must
/// share this geometry with rendering instead of assuming the whole pane.
pub fn preview_terminal_rect(area: Rect, snapshot: &TerminalViewportSnapshot) -> Rect {
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    let width = snapshot.cols.min(inner.width);
    Rect {
        x: inner.x + (inner.width.saturating_sub(width)) / 2,
        y: inner.y,
        width: width.max(1),
        height: inner.height,
    }
}

fn relative_date(created_at_ms: u64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mins = now_ms.saturating_sub(created_at_ms) / 60_000;
    match mins {
        0..=59 => format!("{mins}m"),
        60..=1439 => format!("{}h", mins / 60),
        _ => format!("{}d", mins / 1440),
    }
}

/// The timestamp a row's age should reflect: last activity, falling back to
/// creation when a session has never produced output. Showing `created_at`
/// made a session used minutes ago read as hours old.
pub fn row_age_ms(activity_at: u64, created_at: u64) -> u64 {
    if activity_at > 0 {
        activity_at.max(created_at)
    } else {
        created_at
    }
}

/// Truncate to `max` display cells with a trailing ellipsis (char-count
/// approximation of width).
fn ellipsize(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = chars[..max - 1].iter().collect();
    out.push('…');
    out
}

/// Selection styling that survives any terminal theme: a solid
/// opposite-pole bar rather than a dark tint (which disappears against
/// Ghostty's default background).
fn selected_marker(selected: bool) -> Span<'static> {
    // Keeps the leading cell's width identical either way so rows don't
    // shift as the selection moves.
    Span::raw(if selected { "▌" } else { " " })
}

/// Paint a whole row as the selection: a solid light bar with dark text.
/// Span styles beat a line-level style in ratatui, so each span has to be
/// rewritten — setting the style on the `Line` alone left the per-span
/// colors (status glyph, muted labels) fighting the highlight, which is why
/// a subtle tint read as "no selection at all" on Ghostty's dark default.
fn apply_selection(line: Line<'static>) -> Line<'static> {
    // Rows with no CLI identity go monochrome: white bar on dark
    // terminals, black bar on light ones, ink at the opposite pole.
    let (bar, ink) = if light_background() {
        (Color::Rgb(15, 17, 21), Color::Rgb(245, 246, 250))
    } else {
        (Color::Rgb(240, 242, 247), Color::Rgb(10, 12, 18))
    };
    paint_selection(line, bar, ink)
}

/// Session-shaped rows carry their CLI's accent — the same color the
/// spinner uses — so the selection says *which agent* at a glance. Every
/// provider accent is mid-bright, so one dark ink reads on all of them.
fn apply_cli_selection(line: Line<'static>, command: &str) -> Line<'static> {
    if command.is_empty() {
        return apply_selection(line);
    }
    paint_selection(line, cli_color(command), Color::Rgb(10, 12, 18))
}

fn paint_selection(line: Line<'static>, bar: Color, ink: Color) -> Line<'static> {
    let spans = line
        .spans
        .into_iter()
        .map(|span| {
            let bold = span.style.add_modifier.contains(Modifier::BOLD);
            let mut style = Style::default().bg(bar).fg(ink);
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            Span::styled(span.content, style)
        })
        .collect::<Vec<_>>();
    Line::from(spans).style(Style::default().bg(bar))
}

/// Pad a line with trailing spaces to `width` columns, so a selection bar
/// painted over it spans the full row instead of stopping at the text —
/// the same reason `action_row` pads (a bar that stops mid-row reads as a
/// fragment, not a selection).
fn pad_line(mut line: Line<'static>, width: u16) -> Line<'static> {
    let used: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (width as usize).saturating_sub(used);
    if pad > 0 {
        line.spans.push(Span::raw(" ".repeat(pad)));
    }
    line
}

/// A full-width actionable row: "+ New session", worktree folder rows,
/// "+ Add project". Padding to the sidebar's width is what
/// makes the selection bar span the row like a session's does — without it
/// the highlight stops at the text and reads as a fragment, not a button.
/// `trailing` is right-aligned, for hints like the active project's (n).
fn action_row(
    selected: bool,
    hovered: bool,
    width: u16,
    spans: Vec<Span<'static>>,
    trailing: &str,
) -> Line<'static> {
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let pad = (width as usize).saturating_sub(used + trailing.chars().count() + 1);
    let mut spans = spans;
    spans.push(Span::raw(" ".repeat(pad)));
    if !trailing.is_empty() {
        spans.push(Span::styled(
            trailing.to_string(),
            Style::default().fg(MUTED),
        ));
    }
    spans.push(Span::raw(" "));
    let line = Line::from(spans);
    if selected {
        apply_selection(line)
    } else if hovered {
        line.style(Style::default().bg(hover_tint()))
    } else {
        line
    }
}

/// The desktop's `ProjectFolderColor` palette (Theme.swift), by rawValue.
/// Light/dark hex pairs match the app exactly; the terminal's background
/// picks the variant the same way macOS appearance does.
fn project_folder_color(raw: &str) -> Option<Color> {
    let (light, dark): (u32, u32) = match raw {
        "sky" => (0x2095C9, 0x7DD3FC),
        "blue" => (0x4F73E6, 0x7EA6FF),
        "violet" => (0x7B5BDA, 0xB79CFF),
        "rose" => (0xD75F8F, 0xF79AC0),
        "amber" => (0xB87511, 0xF8C86A),
        "moss" => (0x5F9A3D, 0x9DD67A),
        "teal" => (0x159B91, 0x64DCCB),
        "graphite" => (0x687083, 0xB8BCC8),
        _ => return None,
    };
    let hex = if light_background() { light } else { dark };
    Some(Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8))
}

/// Ink for a chevron sitting on a folder color: the palette's light-mode
/// hexes are dark and the dark-mode ones bright, so the ink is simply the
/// opposite pole — same reasoning as `apply_selection`.
fn folder_ink() -> Color {
    if light_background() {
        Color::Rgb(245, 246, 250)
    } else {
        Color::Rgb(10, 12, 18)
    }
}

/// Mouse-hover wash for a sidebar row: a muted gray, a step quieter than the
/// carried-block tint so it never reads as a selection. Shared by session
/// rows, project headers, and worktree/group folder rows.
fn hover_tint() -> Color {
    if light_background() {
        Color::Rgb(222, 226, 233)
    } else {
        Color::Rgb(44, 48, 56)
    }
}

fn sidebar_line(
    app: &App,
    item: &SidebarItem,
    selected: bool,
    hovered: bool,
    width: u16,
) -> Line<'static> {
    match item {
        SidebarItem::Header(name) => {
            // Project names carry near-default ink: they are structure, not
            // chrome. A group with a working session shimmers, so activity
            // stays visible when the group is collapsed (desktop parity).
            let arrow = if app.collapsed.contains(name) {
                "▸"
            } else {
                "▾"
            };
            // A folder color set in the desktop app paints the chevron's
            // cell — the TUI's version of the tinted folder icon. The
            // palette is display-only here; it is assigned in the app.
            let arrow_style = match app
                .project_color_for_header(name)
                .as_deref()
                .and_then(project_folder_color)
            {
                Some(bg) => Style::default().bg(bg).fg(folder_ink()),
                None => Style::default().fg(MUTED),
            };
            let mut spans = vec![
                Span::styled(arrow.to_string(), arrow_style),
                Span::styled(" ", Style::default().fg(MUTED)),
            ];
            if app.group_is_busy(name) {
                spans.extend(shimmer_spans(name, HEADER));
            } else {
                spans.push(Span::styled(
                    name.clone(),
                    Style::default().fg(HEADER).add_modifier(Modifier::BOLD),
                ));
            }
            // Hovering a header offers "+ New" at the right edge — the mouse
            // way into a new session now that most projects have no row
            // for it. Clicks land there whether or not it is painted (see
            // HEADER_ADD_ZONE in main).
            if hovered {
                let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
                let pad = (width as usize).saturating_sub(used + HEADER_ADD_LABEL.chars().count());
                if pad > 0 {
                    spans.push(Span::raw(" ".repeat(pad)));
                    spans.push(Span::styled(HEADER_ADD_LABEL, Style::default().fg(MUTED)));
                }
            }
            let line = Line::from(spans);
            if hovered {
                // Same wash as session rows so the whole row lights up under
                // the cursor, not just "+ New". Span-level styles (the tinted
                // chevron cell) keep their own bg over this base.
                line.style(Style::default().bg(hover_tint()))
            } else {
                line
            }
        }
        SidebarItem::WorktreeHeader {
            project_id,
            name,
            branch,
            count,
            is_group,
            ..
        } => {
            // A collapsible folder row under the parent's header. Click
            // toggles every folder; only Git worktrees enter keyboard
            // selection and use ⏎. Plain groups remain structural headers.
            // A collapsed folder still shows life (shimmer) and attention
            // (◆), same as a collapsed project header.
            let open = app.expanded_worktrees.contains(project_id);
            let arrow = if open { "▾" } else { "▸" };
            let mut spans = vec![
                selected_marker(selected),
                // The chevron owns the child-row gutter. With no extra pad,
                // a plain group's label starts on the same column as the
                // parent project's normal session labels.
                Span::styled(arrow.to_string(), Style::default().fg(MUTED)),
            ];
            if *is_group {
                // A plain group: no checkout, no branch glyph.
                spans.push(Span::styled(" ".to_string(), Style::default().fg(MUTED)));
            } else {
                // Branch glyph after the arrow: this is a checkout of the
                // project above it, not another project.
                spans.push(Span::styled(" ⎇ ".to_string(), Style::default().fg(MUTED)));
            }
            if app.worktree_is_busy(project_id) {
                spans.extend(shimmer_spans(name, HEADER));
            } else {
                spans.push(Span::styled(name.clone(), Style::default().fg(HEADER)));
            }
            if !open && app.worktree_needs_attention(project_id) {
                spans.push(Span::styled(
                    " ◆",
                    Style::default().fg(status_color(Status::Attention)),
                ));
            }
            // Right edge, date-style: a parenthesized count in quiet ink,
            // prefixed by the branch only when it says something the name
            // doesn't. Hover replaces the cluster with "+ New" (new session in
            // this folder), the same affordance a project header offers; the
            // click zone works whether or not it is painted.
            let trailing = if hovered {
                vec![right_flush_folder_badge(HEADER_ADD_LABEL)]
            } else {
                let mut trailing = Vec::new();
                if !branch.is_empty() && branch != name {
                    trailing.push(Span::styled(
                        format!("{branch} "),
                        Style::default().fg(MUTED),
                    ));
                }
                trailing.push(folder_count(*count));
                trailing
            };
            // Flush to the right edge (no action_row margin column) so the
            // count lines up with the session rows' date column.
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            let trailing_width: usize = trailing
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            let pad = (width as usize).saturating_sub(used + trailing_width);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.extend(trailing);
            let line = Line::from(spans);
            if selected {
                apply_selection(line)
            } else if hovered {
                line.style(Style::default().bg(hover_tint()))
            } else {
                line
            }
        }
        SidebarItem::AddProject => action_row(
            selected,
            hovered,
            width,
            vec![Span::styled(" + Add project", Style::default().fg(MUTED))],
            "",
        ),
        SidebarItem::NewSession { project, .. } => {
            // Indented like a session so it reads as a child of the folder,
            // not another project. The project `n` targets says so — the key
            // works from anywhere, so which project it means must be visible.
            let is_active = app.active_project_id().as_deref() == Some(project.as_str());
            // An empty folder's row sits one level deeper, exactly where
            // the folder's session labels would start.
            let in_folder = app.model.items.iter().any(|item| {
                matches!(item, SidebarItem::WorktreeHeader { project_id, .. } if project_id == project)
            });
            let mut spans = vec![
                // Three in, so the "+" lines up with the session labels
                // below it (marker gutter + gap) rather than sitting a
                // column proud of them.
                Span::raw(if in_folder { "     " } else { "   " }),
                Span::styled("+ New session", Style::default().fg(MUTED)),
            ];
            if is_active {
                // Beside the label, not out at the edge: it names the key
                // for THIS row, so it belongs with the words.
                spans.push(Span::styled(" (n)", Style::default().fg(MUTED)));
            }
            action_row(selected, hovered, width, spans, "")
        }
        SidebarItem::Session(index) => {
            let row = &app.model.rows[*index];
            let mut label_style = Style::default();
            if !row.running {
                label_style = label_style.fg(MUTED);
            }
            // The age is right-aligned in a fixed field ("5m" vs "14h" would
            // otherwise shift everything left of it), so the pin stars line
            // up down the column.
            const DATE_WIDTH: usize = 4;
            // No hold-to-preview: seeing a bare ctrl press needs a kitty
            // flag that breaks shifted keys (see the protocol comment in
            // main). ^1…^9 is documented in the (?) overlay instead.
            let jump: Option<String> = None;
            let date = format!(
                "{:>DATE_WIDTH$}",
                jump.clone()
                    .unwrap_or_else(|| relative_date(row_age_ms(row.activity_at, row.created_at)))
            );
            // Sessions hang under their project header. The status marker
            // sits out at column 1 — left of the labels, which stay three
            // columns in — so spinners read as a gutter down the list. A
            // worktree's sessions sit one level deeper, under their folder.
            let indent: usize = if app.session_in_worktree(row) { 3 } else { 1 };
            const MARKER: usize = 1;
            const GAP: usize = 1;
            let pin_width = if row.pinned { 2 } else { 0 };
            let label_max = (width as usize)
                .saturating_sub(indent + MARKER + GAP + pin_width + DATE_WIDTH + 1)
                .max(1);
            let label = ellipsize(&row.label, label_max);
            let pad = (width as usize).saturating_sub(
                indent + MARKER + GAP + label.chars().count() + pin_width + DATE_WIDTH,
            );
            let unread = app.unread_ids.contains(&row.id);
            let mut spans = vec![
                Span::raw(" ".repeat(indent)),
                status_span_with_unread(row.status, row.presentation_command(), unread),
                Span::raw(" "),
                Span::styled(label, label_style),
                Span::raw(" ".repeat(pad)),
            ];
            if row.pinned {
                // Pins are a state, not an alert — default ink keeps them
                // from competing with the status markers for attention.
                spans.push(Span::raw("⭑ "));
            }
            spans.push(Span::styled(
                date,
                if jump.is_some() {
                    Style::default().fg(HEADER).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ));
            let line = Line::from(spans);
            if selected {
                apply_cli_selection(line, row.presentation_command())
            } else if hovered {
                // Mouse feedback: a muted gray wash (see hover_tint).
                line.style(Style::default().bg(hover_tint()))
            } else {
                line
            }
        }
    }
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    // Title mirrors the desktop: "Projects" plus an always-present activity
    // control at top-right. It animates whenever ANY session is working, so
    // activity stays visible with every project collapsed; at rest it is a
    // quiet activity ring with a blue unread pip when work is waiting.
    let title = vec![Span::styled(
        " Projects ",
        Style::default().add_modifier(Modifier::BOLD),
    )];
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Line::from(title));
    // Remote Controller scope: the ONLY visible difference from the local
    // UI is this host indicator on the sidebar's bottom edge — green, with
    // the Host's name (docs/plans/host-controller-transports.md, Controller
    // architecture). Everything else renders exactly as Local.
    if let Some(host) = app.feed_note.strip_prefix("remote:") {
        block = block.title_bottom(
            Line::from(Span::styled(
                format!(" {host} "),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ))
            .centered(),
        );
    } else {
        let has_unread = app.activity_menu_entries().iter().any(|entry| entry.unread);
        let mut activity = if app.any_busy() {
            vec![Span::styled(
                format!(" {}", spinner_frame()),
                Style::default().fg(MUTED),
            )]
        } else {
            vec![Span::styled(" ◉".to_string(), Style::default().fg(MUTED))]
        };
        if has_unread {
            activity.push(Span::styled(
                "•",
                Style::default().fg(Color::Rgb(64, 140, 255)),
            ));
        }
        activity.push(Span::raw(" "));
        block = block.title(Line::from(activity).right_aligned());
    }
    let block = block
        // The only permanent chrome: the footer menu, bottom-left. Search,
        // keybindings, and the way out of a focused terminal are all
        // documented behind it (and in the (?) overlay) rather than
        // crowding the frame. Accent ink, not a pill: it should stand out
        // from the DarkGray border without shouting like a selection.
        .title_bottom(
            Line::from(Span::styled(
                MENU_LABEL,
                Style::default().fg(FOCUS).add_modifier(Modifier::BOLD),
            ))
            .left_aligned(),
        )
        // Bottom-right: fold/unfold every project at once (the `-` key's
        // mouse twin). Same accent ink as the menu label.
        .title_bottom(
            Line::from(Span::styled(
                if app.all_headers_collapsed() {
                    UNFOLD_LABEL
                } else {
                    FOLD_LABEL
                },
                Style::default().fg(FOCUS).add_modifier(Modifier::BOLD),
            ))
            .right_aligned(),
        );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }
    // During a drag this is the list as it WOULD BE after dropping, with
    // the carried block already in its new home — so the row visibly moves
    // under the cursor instead of a tint appearing on some other row.
    let render = app.sidebar_render();
    let end = (app.sidebar_scroll + inner.height as usize).min(render.items.len());
    let lines: Vec<Line> = render.items[app.sidebar_scroll..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let pos = app.sidebar_scroll + offset;
            let hovered = app.hovered_sidebar_pos() == Some(pos);
            let mut line = sidebar_line(
                app,
                item,
                render.selected == Some(pos),
                hovered,
                inner.width,
            );
            if let Some((start, end)) = render.carried {
                if pos >= start && pos <= end {
                    // The block you're holding: lifted off the list.
                    let tint = if light_background() {
                        Color::Rgb(206, 211, 219)
                    } else {
                        Color::Rgb(58, 62, 72)
                    };
                    line = line.style(Style::default().bg(tint));
                }
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
    // Overflow indicator: a thumb drawn ON the right border itself — no
    // arrows, no track glyph (the border line is the track), and nothing at
    // all when the list fits. A heavier ┃ in brighter ink over the DarkGray │
    // reads as "where you are" without adding a column of chrome.
    let viewport = inner.height as usize;
    if render.items.len() > viewport {
        // content_length counts scroll POSITIONS, not rows: max position is
        // items - viewport, so the thumb touches the bottom border exactly
        // when the last row is visible.
        let mut state = ScrollbarState::new(render.items.len() - viewport + 1)
            .position(app.sidebar_scroll)
            .viewport_content_length(viewport);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(None)
                .thumb_symbol("┃")
                .thumb_style(Style::default().fg(MUTED)),
            // Vertical margin keeps the ┐/┘ corners and both title rows.
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut state,
        );
    }
}

/// The archive library for a project, shown in the preview pane — the
/// TUI's version of the desktop's archive page. Opened with `a` on the
/// project's selection or via the project context menu's "Archived (N)".
fn draw_archive(f: &mut Frame, area: Rect, app: &App, group: &str, row: usize) {
    let rows = app.archived_matches(group);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" Archive · {group} "))
        .title_bottom(Line::from(Span::styled(
            if rows
                .get(row)
                .is_some_and(|session| session.resume_available)
            {
                " type to search · ⏎ Restore & Resume · x remove "
            } else {
                " type to search · ⏎ Restore · x remove "
            },
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let search = Line::from(vec![
        Span::styled("  search ", Style::default().fg(MUTED)),
        Span::raw(app.archive_query.clone()),
        Span::styled("▏", Style::default().fg(Color::Cyan)),
    ]);
    let split = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);
    f.render_widget(Paragraph::new(vec![search, Line::from("")]), split[0]);
    let inner = split[1];
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                if app.archive_query.is_empty() {
                    "  nothing archived here yet"
                } else {
                    "  no archived session matches"
                },
                Style::default().fg(MUTED),
            ))),
            inner,
        );
        return;
    }
    let visible = inner.height as usize;
    let start = row.saturating_sub(visible.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, session)| {
            let line = Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<44}", ellipsize(&session.label, 44)),
                    Style::default(),
                ),
                Span::styled(
                    format!(
                        "{:<10}",
                        relative_date(row_age_ms(session.activity_at, session.created_at))
                    ),
                    Style::default().fg(MUTED),
                ),
                Span::styled(session.command.clone(), Style::default().fg(MUTED)),
            ]);
            if i == row {
                apply_cli_selection(line, session.presentation_command())
            } else {
                line
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

#[derive(Clone, Copy)]
enum RecentVisualRow {
    Header(&'static str),
    Entry(usize),
}

fn recent_visual_rows(entries: &[crate::RecentActivityEntry]) -> Vec<RecentVisualRow> {
    let active = entries.iter().take_while(|entry| entry.working).count();
    let mut rows = Vec::new();
    if active > 0 {
        rows.push(RecentVisualRow::Header("Active"));
        rows.extend((0..active).map(RecentVisualRow::Entry));
    }
    if active < entries.len() {
        rows.push(RecentVisualRow::Header("Recent"));
        rows.extend((active..entries.len()).map(RecentVisualRow::Entry));
    }
    rows
}

fn recent_visible_start(rows: &[RecentVisualRow], selected: usize, visible: usize) -> usize {
    if rows.len() <= visible || visible == 0 {
        return 0;
    }
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, RecentVisualRow::Entry(index) if *index == selected))
        .unwrap_or(0);
    selected_row
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible))
}

/// Click mapping for the All recent main-pane list. Header rows are
/// structural and return None; removed-session log rows still return their
/// index so the caller can keep selection stable while declining navigation.
pub fn recent_activity_row_at(
    area: Rect,
    app: &App,
    selected: usize,
    col: u16,
    row: u16,
) -> Option<usize> {
    let entries = app.recent_activity_entries();
    let rows = recent_visual_rows(&entries);
    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if col < inner.x
        || col >= inner.x + inner.width
        || row < inner.y
        || row >= inner.y + inner.height
    {
        return None;
    }
    let start = recent_visible_start(&rows, selected, inner.height as usize);
    match rows.get(start + (row - inner.y) as usize) {
        Some(RecentVisualRow::Entry(index)) => Some(*index),
        _ => None,
    }
}

fn draw_recent_activity(f: &mut Frame, area: Rect, app: &App, selected: usize) {
    let entries = app.recent_activity_entries();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Line::from(Span::styled(
            " All recent ",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .title_bottom(Line::from(Span::styled(
            " ↑↓ navigate · ⏎ open · esc close ",
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if entries.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No recent activity",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "Session starts, finishes, and input requests will appear here.",
                    Style::default().fg(MUTED),
                )),
            ])
            .centered(),
            inner,
        );
        return;
    }
    let visual_rows = recent_visual_rows(&entries);
    let start = recent_visible_start(&visual_rows, selected, inner.height as usize);
    let lines = visual_rows
        .iter()
        .skip(start)
        .take(inner.height as usize)
        .map(|visual| match *visual {
            RecentVisualRow::Header(label) => Line::from(Span::styled(
                format!("  {label}"),
                Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
            )),
            RecentVisualRow::Entry(index) => {
                let entry = &entries[index];
                let provider = unpeel_core::integrations::command_head(&entry.command);
                let marker = if entry.working {
                    Span::styled(
                        spinner_frame().to_string(),
                        Style::default().fg(cli_color(&entry.command)),
                    )
                } else if entry.unread {
                    Span::styled("●", Style::default().fg(Color::Rgb(64, 140, 255)))
                } else if entry.session_id.is_some() {
                    Span::styled("●", Style::default().fg(cli_color(&entry.command)))
                } else {
                    Span::styled("·", Style::default().fg(Color::DarkGray))
                };
                let event_width = entry.event.chars().count().min(22);
                let project_width = entry.project.chars().count().min(20);
                let fixed = event_width + project_width + provider.chars().count() + 10;
                let title_max = (inner.width as usize).saturating_sub(fixed).max(8);
                let title = ellipsize(&entry.title, title_max);
                let used = 4
                    + title.chars().count()
                    + provider.chars().count()
                    + project_width
                    + event_width
                    + 5;
                let pad = (inner.width as usize).saturating_sub(used);
                let disabled = entry.session_id.is_none();
                let line = Line::from(vec![
                    Span::raw(" "),
                    marker,
                    Span::raw(" "),
                    Span::styled(
                        title,
                        Style::default().fg(if disabled {
                            Color::DarkGray
                        } else {
                            Color::Reset
                        }),
                    ),
                    Span::raw(" ".repeat(pad + 1)),
                    Span::styled(
                        provider.to_string(),
                        Style::default().fg(cli_color(&entry.command)),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        ellipsize(&entry.project, project_width),
                        Style::default().fg(MUTED),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        ellipsize(&entry.event, event_width),
                        Style::default().fg(MUTED),
                    ),
                    Span::raw(" "),
                ]);
                if index == selected {
                    apply_cli_selection(pad_line(line, inner.width), &entry.command)
                } else {
                    line
                }
            }
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_preview(f: &mut Frame, area: Rect, app: &App, snapshots: &SnapshotService) {
    if let Some(selected) = app.selected_recent {
        draw_recent_activity(f, area, app, selected);
        return;
    }
    if let Some((group, row)) = &app.selected_archive {
        draw_archive(f, area, app, group, *row);
        return;
    }
    let snapshot_for_tag = app.selected_session().and_then(|s| snapshots.get(&s.id));
    let grid_tag = match &snapshot_for_tag {
        Some(s) if s.cols != area.width.saturating_sub(2) => format!(" {}×{} ", s.cols, s.rows),
        _ => String::new(),
    };
    let show_mobile_tag = app
        .selected_session()
        .map(|s| app.mobile_resized(&s.id) && !grid_tag.is_empty())
        .unwrap_or(false);
    let Some(session) = app.selected_session() else {
        f.render_widget(
            Paragraph::new("no session selected")
                .style(Style::default().fg(Color::DarkGray))
                .centered(),
            area,
        );
        return;
    };
    let project = app.project_name_for(&session.id);
    let branch = git_branch_of(&session.cwd);
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(project, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            branch
                .map(|b| format!(" · {b} "))
                .unwrap_or_else(|| " ".into()),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            if app.preview_scroll > 0 {
                format!(" ↑{} ", app.preview_scroll)
            } else {
                String::new()
            },
            Style::default().fg(Color::Magenta),
        ),
        Span::styled(grid_tag, Style::default().fg(Color::DarkGray)),
        Span::styled(
            // Only while a phone resize is recent AND the grid still
            // mismatches the pane — matching sizes need no explanation.
            if show_mobile_tag {
                " Resized for mobile "
            } else {
                ""
            },
            Style::default().fg(Color::Magenta),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if app.terminal_focus {
            Style::default().fg(FOCUS)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);
    // Live local sites the session's project serves (host-probed
    // `detected_local_urls`, aggregated across the project family) paint as
    // a clickable chip on the top border's right end — the TUI's counterpart
    // of the desktop titlebar chip. Painted after the block so it wins the
    // border row; hit-test shares `local_urls_chip_rect`.
    let local_urls = app.local_site_urls();
    if !local_urls.is_empty() {
        if let (Some(rect), Some(label)) = (
            local_urls_chip_rect(area, &local_urls),
            local_urls_chip_label(&local_urls),
        ) {
            f.render_widget(
                Paragraph::new(Span::styled(label, Style::default().fg(Color::Cyan))),
                rect,
            );
        }
    }
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    // A stopped session has no terminal — showing the dead one's last frame
    // invites typing into nothing. Say what it is and how to bring it back.
    if !session.running {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    session.label.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(" is stopped", Style::default().fg(MUTED)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  press "),
                Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(
                    " to resume it — the conversation picks up where it left off",
                    Style::default().fg(MUTED),
                ),
            ]),
            Line::from(vec![
                Span::raw("  press "),
                Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(" to remove it", Style::default().fg(MUTED)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(session.command.clone(), Style::default().fg(MUTED)),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }
    if session.status == Status::Starting && snapshots.get(&session.id).is_none() {
        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!("  {} ", spinner_frame()),
                    Style::default().fg(cli_color(session.presentation_command())),
                ),
                Span::styled("starting ", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(session.command.clone(), Style::default().fg(MUTED)),
            ]),
            Line::from(Span::styled(
                "  the agent is booting — output appears as soon as it writes",
                Style::default().fg(MUTED),
            )),
        ];
        f.render_widget(Paragraph::new(lines), inner);
        return;
    }
    match snapshots.get(&session.id) {
        // A snapshot whose grid is entirely blank, while the session is still
        // *starting*, is the gap between "the host is up" and "the agent has
        // drawn something" — rendering it faithfully means staring at an empty
        // pane with no sign anything is happening. Say so, and get out of the
        // way the moment it writes.
        //
        // This is gated on `Starting` on purpose: once a session is running,
        // agents that redraw by clear-then-repaint (claude et al.) routinely
        // emit a momentarily all-blank frame between the two writes, and the
        // live stream feeds those sub-frames fast enough to catch it. Showing
        // the placeholder on *those* blanks flips the whole pane
        // content→spinner→content every redraw — the "random blinking". A
        // running session's transient blank must render as a blank pane (gone
        // in ~16ms), never the spinner.
        Some(snapshot)
            if session.status == Status::Starting
                && snapshot
                    .viewport_rows
                    .iter()
                    .all(|row| row.text.trim().is_empty()) =>
        {
            let lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        format!("  {} ", spinner_frame()),
                        Style::default().fg(cli_color(session.presentation_command())),
                    ),
                    Span::styled("starting", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(
                        format!(" · {}", session.command),
                        Style::default().fg(MUTED),
                    ),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), inner);
        }
        Some(snapshot) => {
            let selection = app
                .terminal_selection
                .as_ref()
                .filter(|value| value.session_id == session.id)
                // A completed selection belongs to one stable frame. If the
                // child produces more output, resume the live grid instead
                // of freezing the terminal indefinitely under an old copy
                // highlight. During the drag, however, the frozen frame is
                // intentional so output cannot move under the pointer.
                .filter(|value| {
                    value.uses_frozen_snapshot()
                        || value.snapshot.output_offset == snapshot.output_offset
                });
            let snapshot = selection
                .filter(|value| value.uses_frozen_snapshot())
                .map(|value| &value.snapshot)
                .unwrap_or(&snapshot);
            // True-grid content letterboxes into the pane: centered when
            // narrower, clipped when wider — never reflowed.
            let target = preview_terminal_rect(area, snapshot);
            f.render_widget(Paragraph::new(snapshot_lines(snapshot, selection)), target);
            // Real cursor position while the terminal owns the keyboard.
            if app.terminal_focus && app.preview_scroll == 0 {
                let cx = target.x + snapshot.cursor_col.min(target.width.saturating_sub(1));
                let cy = target.y + snapshot.cursor_row;
                if cy < target.y + target.height {
                    f.set_cursor_position((cx, cy));
                }
            }
        }
        None => f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("  {} ", spinner_frame()),
                    Style::default().fg(cli_color(session.presentation_command())),
                ),
                Span::styled("attaching…", Style::default().fg(MUTED)),
            ])),
            inner,
        ),
    }
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App) {
    if !app.has_status_message() {
        return;
    }
    f.render_widget(ratatui::widgets::Clear, area);
    let left = if let Some((_, title)) = app.approvals.front() {
        Line::from(Span::styled(
            format!(" ⚠ {title} y/n "),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ))
    } else if let Some(confirm) = &app.confirm {
        Line::from(Span::styled(
            format!(" {}? y/n ", confirm.prompt),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
    } else if let Some(in_flight) = &app.in_flight {
        Line::from(vec![
            Span::styled(
                format!(" {} ", spinner_frame()),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(format!("{}… ", in_flight.label), Style::default()),
        ])
    } else if app.selection_mode {
        // A mode that changes what the mouse does has to announce itself —
        // that is state, not a hint.
        Line::from(Span::styled(
            " selection mode — drag to select/copy · v to return ",
            Style::default().fg(MUTED),
        ))
    } else {
        // Nothing to say: `draw` gives this row no height at all.
        return;
    };
    f.render_widget(Paragraph::new(left), area);
}

/// Outcome toast, top-right: "removed", "renamed", errors — anything a verb
/// reports after the fact. A pill, not a bar: it overlays one line of the
/// corner briefly (TOAST_TTL, or the next keypress) and never resizes the
/// layout. Interactive states (approvals, confirms, the in-flight spinner,
/// selection mode) stay on the bottom row — they wait for the user; a toast
/// only informs.
fn draw_toast(f: &mut Frame, area: Rect, app: &App) {
    let (text, bg, fg) = if let Some(info) = &app.info {
        // Genuine failures shout; outcomes stay quiet.
        let failed = info.contains("failed")
            || info.contains("error")
            || info.contains("not reachable")
            || info.contains("unavailable");
        let (bg, fg) = if failed {
            (Color::Rgb(122, 42, 42), Color::Rgb(250, 236, 236))
        } else if light_background() {
            (Color::Rgb(206, 211, 219), Color::Rgb(25, 28, 35))
        } else {
            (Color::Rgb(58, 62, 72), Color::Rgb(230, 232, 238))
        };
        (format!(" {info} "), bg, fg)
    } else if let Some(version) = &app.update_available {
        // The persistent update notice: waits behind any transient toast,
        // stays until clicked (main.rs persists the dismissal per version).
        let (bg, fg) = if light_background() {
            (Color::Rgb(186, 205, 231), Color::Rgb(18, 34, 58))
        } else {
            (Color::Rgb(42, 66, 100), Color::Rgb(222, 234, 250))
        };
        (update_toast_text(version), bg, fg)
    } else if let Some(hint) = &app.env_hint {
        // A one-time environment tip (e.g. Herdr right-click passthrough):
        // same slot and tone as the update notice, waits behind it, click
        // dismisses (marker file).
        let (bg, fg) = if light_background() {
            (Color::Rgb(186, 205, 231), Color::Rgb(18, 34, 58))
        } else {
            (Color::Rgb(42, 66, 100), Color::Rgb(222, 234, 250))
        };
        (format!(" {} ", hint.text), bg, fg)
    } else {
        return;
    };
    let Some(rect) = toast_rect(area, text.chars().count() as u16) else {
        return;
    };
    f.render_widget(ratatui::widgets::Clear, rect);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text,
            Style::default().bg(bg).fg(fg),
        ))),
        rect,
    );
}

/// Where a toast of `text_chars` cells lands in `area` — one formula for the
/// draw and the update-toast mouse hit-test.
fn toast_rect(area: Rect, text_chars: u16) -> Option<Rect> {
    let width = text_chars.min(area.width.saturating_sub(4));
    if width == 0 || area.height < 2 {
        return None;
    }
    Some(Rect {
        x: area.x + area.width.saturating_sub(width + 2),
        y: area.y + 1,
        width,
        height: 1,
    })
}

fn update_toast_text(version: &str) -> String {
    format!(" ⬆ unpeel {version} available — click to dismiss ")
}

/// Whether a click at (`column`, `row`) lands on the update toast.
pub fn update_toast_hit(area: Rect, version: &str, column: u16, row: u16) -> bool {
    let text = update_toast_text(version);
    toast_rect(area, text.chars().count() as u16)
        .is_some_and(|rect| row == rect.y && column >= rect.x && column < rect.x + rect.width)
}

/// Whether a click at (`column`, `row`) lands on the one-time environment
/// tip toast showing `text` (padded to match the draw).
pub fn hint_toast_hit(area: Rect, text: &str, column: u16, row: u16) -> bool {
    toast_rect(area, (text.chars().count() + 2) as u16)
        .is_some_and(|rect| row == rect.y && column >= rect.x && column < rect.x + rect.width)
}

/// The preset picker's frame — centered for keyboard entry, anchored below
/// a clicked `+` for mouse entry. Shared by rendering and hit-testing.
pub fn preset_picker_rect(
    area: Rect,
    presets: &[(String, String)],
    target: &str,
    anchor: Option<(u16, u16)>,
) -> Rect {
    let height = (presets.len() as u16 + 2)
        .min(if anchor.is_some() {
            area.height
        } else {
            area.height.saturating_sub(4)
        })
        .max(3)
        .min(area.height.max(1));
    let widest_row = presets
        .iter()
        .map(|(label, command)| {
            let command_width = if label != command && command != crate::MANAGE_PRESETS_COMMAND {
                command.chars().count() + 1
            } else {
                0
            };
            // Selection marker/padding plus the visible label and command.
            label.chars().count() + command_width + 4
        })
        .max()
        .unwrap_or(20) as u16;
    let title_width = if anchor.is_some() {
        format!(" pick a preset · {target} ").chars().count() as u16 + 2
    } else {
        60
    };
    let available_width = if anchor.is_some() {
        area.width
    } else {
        area.width.saturating_sub(4)
    };
    let width = widest_row
        .max(title_width)
        .clamp(20, 60)
        .min(available_width.max(1));
    if let Some((anchor_x, anchor_y)) = anchor {
        let rightmost_x = area.x + area.width.saturating_sub(width);
        let bottommost_y = area.y + area.height.saturating_sub(height);
        Rect {
            x: anchor_x.max(area.x).min(rightmost_x),
            y: anchor_y.max(area.y).min(bottommost_y),
            width,
            height,
        }
    } else {
        Rect {
            x: area.x + (area.width.saturating_sub(width)) / 2,
            y: area.y + (area.height.saturating_sub(height)) / 3,
            width,
            height,
        }
    }
}

/// Which preset a click at (column, row) lands on, mirroring the scroll
/// offset `draw_preset_picker` applies. None = border or outside the list.
pub fn preset_picker_row_at(
    area: Rect,
    presets: &[(String, String)],
    target: &str,
    selected: usize,
    anchor: Option<(u16, u16)>,
    column: u16,
    row: u16,
) -> Option<usize> {
    let rect = preset_picker_rect(area, presets, target, anchor);
    let inner = Rect {
        x: rect.x + 1,
        y: rect.y + 1,
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    };
    if column < inner.x
        || column >= inner.x + inner.width
        || row < inner.y
        || row >= inner.y + inner.height
    {
        return None;
    }
    let first = selected.saturating_sub(inner.height.saturating_sub(1) as usize);
    let index = first + (row - inner.y) as usize;
    (index < presets.len()).then_some(index)
}

fn draw_preset_picker(
    f: &mut Frame,
    area: Rect,
    presets: &[(String, String)],
    selected: usize,
    target: &str,
    anchor: Option<(u16, u16)>,
) {
    let rect = preset_picker_rect(area, presets, target, anchor);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(if anchor.is_some() {
            format!(" pick a preset · {target} ")
        } else {
            format!(" new session in {target} — pick a preset ")
        });
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = presets
        .iter()
        .enumerate()
        .skip(selected.saturating_sub(inner.height.saturating_sub(1) as usize))
        .take(inner.height as usize)
        .map(|(i, (label, command))| {
            let mut spans = if command == crate::MANAGE_PRESETS_COMMAND {
                // Footer shortcut into Settings ▸ Presets, not a launch row.
                vec![Span::styled(
                    format!(" ⚙ {label} "),
                    Style::default().fg(Color::DarkGray),
                )]
            } else {
                vec![Span::styled(
                    format!(" {label} "),
                    Style::default().fg(cli_color(command)),
                )]
            };
            if label != command && command != crate::MANAGE_PRESETS_COMMAND {
                spans.push(Span::styled(
                    command.clone(),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            let mut line = Line::from(spans);
            if i == selected {
                line = line.style(Style::default().bg(Color::Rgb(50, 50, 60)));
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Recent-sessions palette (the desktop's Cmd+K), filtered as you type.
/// The unfiltered view renders `palette_sections` — active and unread
/// sessions from every project, then the current project's, then a
/// projects switcher — under dim captions; the captions come from the
/// same call that feeds `palette_matches("")`, so captions and selection
/// can never disagree. Typing collapses to the flat fuzzy ranking — tiers
/// mean nothing in a scored list.
fn draw_recents(f: &mut Frame, area: Rect, app: &App, query: &str, selected: usize) {
    let matches = app.palette_matches(query);
    enum Row {
        Caption(String),
        Item(usize, crate::palette::Item),
    }
    let mut display: Vec<Row> = Vec::with_capacity(matches.len() + 5);
    if query.trim().is_empty() {
        let mut index = 0usize;
        'sections: for (caption, rows) in app.palette_sections() {
            // No leading divider when the list opens straight on
            // non-session rows (a fresh install with zero sessions).
            if !(caption.is_empty() && display.is_empty()) {
                display.push(Row::Caption(caption));
            }
            for item in rows {
                // `palette_matches("")` caps the flattened list; past that
                // cap the indices would no longer line up.
                if index >= matches.len() {
                    break 'sections;
                }
                display.push(Row::Item(index, item));
                index += 1;
            }
        }
    } else {
        display.extend(
            matches
                .iter()
                .enumerate()
                .map(|(i, item)| Row::Item(i, item.clone())),
        );
    }
    let width = (area.width * 3 / 4).clamp(30, 110);
    let height = ((display.len() as u16).min(15) + 4).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 2,
        width,
        height,
    };
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" command palette ")
        .title_bottom(Line::from(Span::styled(
            " ⏎ open · esc close ",
            Style::default().fg(Color::DarkGray),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::raw(query.to_string()),
        Span::styled("▏", Style::default().fg(Color::Cyan)),
    ])];
    let visible = inner.height.saturating_sub(1) as usize;
    let selected_display = display
        .iter()
        .position(|row| matches!(row, Row::Item(i, _) if *i == selected))
        .unwrap_or(0);
    let start = selected_display.saturating_sub(visible.saturating_sub(1));
    for row in display.iter().skip(start).take(visible) {
        let (i, item) = match row {
            Row::Caption(label) => {
                let text = if label.is_empty() {
                    "  ─────".to_string()
                } else {
                    format!("  {label}")
                };
                lines.push(Line::from(Span::styled(
                    text,
                    Style::default().fg(Color::DarkGray),
                )));
                continue;
            }
            Row::Item(i, item) => (*i, item),
        };
        let session = match &item.action {
            crate::palette::Action::SelectSession(id) => {
                app.model.rows.iter().find(|r| r.id == *id)
            }
            _ => None,
        };
        let marker = match session {
            Some(row) => status_span_with_unread(
                row.status,
                row.presentation_command(),
                app.unread_ids.contains(&row.id),
            ),
            None if item.icon_command.is_empty() => Span::raw("  "),
            None => Span::styled("▸ ", Style::default().fg(cli_color(&item.icon_command))),
        };
        let title_style = match session {
            Some(row) if !row.running => Style::default().fg(Color::DarkGray),
            _ => Style::default(),
        };
        let mut line = Line::from(vec![
            Span::raw("  "),
            marker,
            Span::raw(" "),
            Span::styled(format!("{:<42}", ellipsize(&item.title, 42)), title_style),
            Span::styled(
                format!("{:<22}", ellipsize(&item.subtitle, 22)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(item.kind.label(), Style::default().fg(Color::DarkGray)),
        ]);
        if i == selected {
            line = apply_cli_selection(line, &item.icon_command);
        }
        lines.push(line);
    }
    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no matches",
            Style::default().fg(Color::DarkGray),
        )));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Every binding, grouped — the hint bar can only ever show a slice.
fn draw_help(f: &mut Frame, area: Rect) {
    const GROUPS: [(&str, &[(&str, &str)]); 4] = [
        (
            "sessions",
            &[
                ("↑↓ / click", "select"),
                ("⏎", "type into the terminal"),
                ("ctrl+]", "back to the sidebar"),
                ("drag text", "select + copy without leaving the terminal"),
                ("wheel", "scroll terminal under pointer · sidebar list"),
                ("n", "new session (preset picker)"),
                ("s / r / x", "stop · resume agent/session · remove"),
                ("e / p", "rename · pin"),
                ("a", "project archive (esc closes)"),
            ],
        ),
        (
            "navigation",
            &[
                ("v", "selection mode fallback (release mouse capture)"),
                ("R", "all recent activity"),
                ("ctrl+k or /", "command palette"),
                (
                    "ctrl+1…9",
                    "jump to a session in this project (hold ctrl to see)",
                ),
                ("+ / -", "add project · fold all"),
                ("drag divider", "resize the sidebar"),
                (", ", "settings"),
            ],
        ),
        (
            "settings",
            &[
                ("tab / ←→", "switch section"),
                ("⏎ / space", "activate the selected row"),
                ("J / K", "reorder presets"),
                ("*", "quick-launch star"),
            ],
        ),
        (
            "remote",
            &[
                ("m", "share this host (QR)"),
                ("settings ▸ remote", "paired devices, unpair"),
                ("y / n", "answer an approval"),
            ],
        ),
    ];
    let width = 66.min(area.width);
    let rows: usize = GROUPS.iter().map(|(_, keys)| keys.len() + 2).sum();
    let height = ((rows as u16) + 2).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" keys ")
        .title_bottom(Line::from(Span::styled(
            " any key closes ",
            Style::default().fg(Color::DarkGray),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let mut lines = Vec::new();
    for (group, keys) in GROUPS {
        lines.push(Line::from(Span::styled(
            format!(" {group}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        for (key, description) in keys {
            lines.push(Line::from(vec![
                Span::styled(format!("   {key:<18}"), Style::default()),
                Span::styled(*description, Style::default().fg(Color::Gray)),
            ]));
        }
        lines.push(Line::from(""));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

#[derive(Clone, Copy, Debug)]
struct RenameRow {
    start: usize,
    end: usize,
}

fn rename_rows(value: &str, width: u16) -> Vec<RenameRow> {
    let chars: Vec<char> = value.chars().collect();
    let width = width.max(1) as usize;
    let mut rows = Vec::new();
    let mut start = 0;
    let mut used = 0;
    for (index, ch) in chars.iter().enumerate() {
        let char_width = ch.width().unwrap_or(0);
        if used > 0 && used + char_width > width {
            rows.push(RenameRow { start, end: index });
            start = index;
            used = 0;
        }
        used += char_width;
    }
    rows.push(RenameRow {
        start,
        end: chars.len(),
    });
    // A caret after a completely full last row belongs at column zero of a
    // new visual row, not one cell beyond the field's right edge.
    if !chars.is_empty() && used >= width {
        rows.push(RenameRow {
            start: chars.len(),
            end: chars.len(),
        });
    }
    rows
}

fn rename_cursor_row(rows: &[RenameRow], cursor: usize) -> usize {
    rows.iter()
        .enumerate()
        .find_map(|(index, row)| {
            if cursor < row.end || (index + 1 == rows.len() && cursor <= row.end) {
                Some(index)
            } else {
                None
            }
        })
        .unwrap_or_else(|| rows.len().saturating_sub(1))
}

fn rename_row_col(value: &str, row: RenameRow, position: usize) -> u16 {
    value
        .chars()
        .skip(row.start)
        .take(position.min(row.end).saturating_sub(row.start))
        .map(|ch| ch.width().unwrap_or(0) as u16)
        .sum()
}

/// Geometry of the wrapped rename editor. It grows with the title until it
/// reaches the terminal height; the cursor's visual row is kept in view.
pub fn rename_prompt_rect(area: Rect, input: &crate::RenameInput) -> Rect {
    let width = 66.min(area.width).max(1);
    let text_width = width.saturating_sub(6).max(1);
    let row_count = rename_rows(&input.buffer, text_width).len() as u16;
    let height = (row_count + 4).max(5).min(area.height.max(1));
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    }
}

fn rename_text_rect(area: Rect, input: &crate::RenameInput) -> Rect {
    let rect = rename_prompt_rect(area, input);
    Rect {
        x: rect.x.saturating_add(3),
        y: rect.y.saturating_add(2),
        width: rect.width.saturating_sub(6),
        height: rect.height.saturating_sub(4),
    }
}

fn rename_first_visible_row(input: &crate::RenameInput, rows: &[RenameRow], height: u16) -> usize {
    let cursor_row = rename_cursor_row(rows, input.cursor);
    let visible = height.max(1) as usize;
    cursor_row
        .saturating_sub(visible.saturating_sub(1))
        .min(rows.len().saturating_sub(visible))
}

fn rename_index_in_row(value: &str, row: RenameRow, column: u16) -> usize {
    let mut used = 0;
    for (index, ch) in value
        .chars()
        .enumerate()
        .skip(row.start)
        .take(row.end.saturating_sub(row.start))
    {
        let next = used + ch.width().unwrap_or(0) as u16;
        if column < next {
            return index;
        }
        used = next;
    }
    row.end
}

/// Character position under a mouse cell in the rename field.
pub fn rename_text_index_at(
    area: Rect,
    input: &crate::RenameInput,
    column: u16,
    row: u16,
) -> Option<usize> {
    let text = rename_text_rect(area, input);
    if text.width == 0
        || text.height == 0
        || column < text.x
        || column >= text.x + text.width
        || row < text.y
        || row >= text.y + text.height
    {
        return None;
    }
    let rows = rename_rows(&input.buffer, text.width);
    let first = rename_first_visible_row(input, &rows, text.height);
    let visual_row = first + (row - text.y) as usize;
    let wrapped = *rows.get(visual_row)?;
    Some(rename_index_in_row(&input.buffer, wrapped, column - text.x))
}

/// Nearest character position while a drag leaves the text field. Keeping
/// the gesture captured lets users select to the start/end in one sweep.
pub fn rename_text_index_nearest(
    area: Rect,
    input: &crate::RenameInput,
    column: u16,
    row: u16,
) -> usize {
    let text = rename_text_rect(area, input);
    if text.width == 0 || text.height == 0 {
        return input.cursor;
    }
    let rows = rename_rows(&input.buffer, text.width);
    let first = rename_first_visible_row(input, &rows, text.height);
    let last = (first + text.height as usize)
        .min(rows.len())
        .saturating_sub(1);
    if row < text.y {
        return rows[first].start;
    }
    if row >= text.y + text.height {
        return rows[last].end;
    }
    let visual_row = (first + (row - text.y) as usize).min(last);
    let wrapped = rows[visual_row];
    if column < text.x {
        wrapped.start
    } else if column >= text.x + text.width {
        wrapped.end
    } else {
        rename_index_in_row(&input.buffer, wrapped, column - text.x)
    }
}

fn rename_line(value: &str, row: RenameRow, selection: Option<(usize, usize)>) -> Line<'static> {
    let selected_style = Style::default()
        .bg(Color::Rgb(64, 88, 138))
        .fg(Color::White);
    let mut spans = Vec::new();
    let mut run = String::new();
    let mut run_selected = None;
    for (index, ch) in value
        .chars()
        .enumerate()
        .skip(row.start)
        .take(row.end.saturating_sub(row.start))
    {
        let selected = selection.is_some_and(|(start, end)| index >= start && index < end);
        if run_selected.is_some_and(|current| current != selected) {
            let text = std::mem::take(&mut run);
            spans.push(if run_selected == Some(true) {
                Span::styled(text, selected_style)
            } else {
                Span::raw(text)
            });
        }
        run_selected = Some(selected);
        run.push(ch);
    }
    if !run.is_empty() {
        spans.push(if run_selected == Some(true) {
            Span::styled(run, selected_style)
        } else {
            Span::raw(run)
        });
    }
    Line::from(spans)
}

fn draw_rename_prompt(f: &mut Frame, area: Rect, input: &crate::RenameInput) {
    let rect = rename_prompt_rect(area, input);
    let text = rename_text_rect(area, input);
    let rows = rename_rows(&input.buffer, text.width);
    let first = rename_first_visible_row(input, &rows, text.height);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" rename session ")
        .title_bottom(Line::from(Span::styled(
            " drag select · ⌫ delete · ⏎ save · esc cancel ",
            Style::default().fg(MUTED),
        )));
    f.render_widget(block, rect);
    let lines = rows
        .iter()
        .copied()
        .skip(first)
        .take(text.height as usize)
        .map(|row| rename_line(&input.buffer, row, input.selection()))
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), text);

    let cursor_row = rename_cursor_row(&rows, input.cursor);
    if cursor_row >= first && cursor_row < first + text.height as usize && text.width > 0 {
        let wrapped = rows[cursor_row];
        let column =
            rename_row_col(&input.buffer, wrapped, input.cursor).min(text.width.saturating_sub(1));
        f.set_cursor_position((text.x + column, text.y + (cursor_row - first) as u16));
    }
}

/// A single-field prompt in the same frame as the other dialogs. Rename has
/// its own wrapped editor above; this compact version serves the group name.
fn draw_text_prompt(f: &mut Frame, area: Rect, title: &str, hint: &str, value: &str) {
    let width = 66.min(area.width);
    let height = 5.min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    };
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(format!(" {title} "))
        .title_bottom(Line::from(Span::styled(
            format!(" {hint} "),
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                Span::raw(value.to_string()),
                Span::styled("▏", Style::default().fg(Color::Cyan)),
            ]),
        ]),
        inner,
    );
}

/// Geometry of the add-project dialog. Drawing and mouse hit-testing both
/// derive from this one rect so a click always lands on what was painted.
pub fn project_input_rect(area: Rect, input: &crate::ProjectInput) -> Rect {
    let width = 78.min(area.width);
    let list_height = (input.matches.len() as u16).min(10);
    let height = (list_height + 6).min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    }
}

/// The "[ add ] " chip on the dialog's bottom border, right-aligned like the
/// settings [+]. One constant feeds both the drawing and the hit zone.
const PROJECT_INPUT_ADD_CHIP: u16 = 8; // "[ add ] "

/// Which completion row a click at (column, row) lands on, mirroring the
/// scroll offset `draw_project_input` applies. None = border or elsewhere.
pub fn project_input_row_at(
    area: Rect,
    input: &crate::ProjectInput,
    column: u16,
    row: u16,
) -> Option<usize> {
    let rect = project_input_rect(area, input);
    let inner = Rect {
        x: rect.x + 1,
        y: rect.y + 1,
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    };
    let list_height = (input.matches.len() as u16)
        .min(10)
        .min(inner.height.saturating_sub(2));
    // Line 0 is the typed path, line 1 the blank spacer.
    let top = inner.y + 2;
    if column < inner.x || column >= inner.x + inner.width || row < top || row >= top + list_height
    {
        return None;
    }
    let start = input
        .selected
        .saturating_sub(list_height.saturating_sub(1) as usize);
    let index = start + (row - top) as usize;
    (index < input.matches.len()).then_some(index)
}

/// True when (column, row) hits the [ add ] chip on the bottom border.
pub fn project_input_add_hit(
    area: Rect,
    input: &crate::ProjectInput,
    column: u16,
    row: u16,
) -> bool {
    let rect = project_input_rect(area, input);
    if rect.width < PROJECT_INPUT_ADD_CHIP + 2 || rect.height < 2 {
        return false;
    }
    // Right-aligned bottom titles end just before the corner cell.
    let end = rect.x + rect.width - 1;
    row == rect.y + rect.height - 1 && column >= end - PROJECT_INPUT_ADD_CHIP && column < end
}

/// True anywhere on the dialog, border included — a click outside cancels.
pub fn project_input_frame_hit(
    area: Rect,
    input: &crate::ProjectInput,
    column: u16,
    row: u16,
) -> bool {
    let rect = project_input_rect(area, input);
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Add-project dialog: the path being typed plus the directories it can
/// complete to, so you can walk to a project instead of recalling its path.
/// Fully mouse-driven too: hover tints a row, click selects, double-click
/// descends, and the [ add ] chip commits the selected folder.
fn draw_project_input(
    f: &mut Frame,
    area: Rect,
    input: &crate::ProjectInput,
    mouse: Option<(u16, u16)>,
) {
    let rect = project_input_rect(area, input);
    let list_height = (input.matches.len() as u16).min(10);
    f.render_widget(ratatui::widgets::Clear, rect);
    let hovered_row = mouse.and_then(|(col, row)| project_input_row_at(area, input, col, row));
    let add_hovered = mouse.is_some_and(|(col, row)| project_input_add_hit(area, input, col, row));
    let add_style = if add_hovered {
        Style::default().bg(hover_tint())
    } else {
        Style::default()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" add a project ")
        .title_bottom(Line::from(Span::styled(
            " tab completes · ↑↓ pick · ⏎ add · esc cancel ",
            Style::default().fg(MUTED),
        )))
        .title_bottom(
            Line::from(vec![
                Span::styled("[", add_style.fg(MUTED)),
                Span::styled(
                    " add ",
                    add_style.fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled("] ", add_style.fg(MUTED)),
            ])
            .right_aligned(),
        );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let home = std::env::var("HOME").unwrap_or_default();
    let shown = if !home.is_empty() && input.query.starts_with(&home) {
        input.query.replacen(&home, "~", 1)
    } else {
        input.query.clone()
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::raw(shown),
            Span::styled("▏", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
    ];
    if input.matches.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no folders here",
            Style::default().fg(MUTED),
        )));
    }
    let start = input
        .selected
        .saturating_sub(list_height.saturating_sub(1) as usize);
    for (i, path) in input
        .matches
        .iter()
        .enumerate()
        .skip(start)
        .take(list_height as usize)
    {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let is_repo = std::path::Path::new(path).join(".git").exists();
        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{name:<34}"), Style::default()),
            Span::styled(if is_repo { "git" } else { "" }, Style::default().fg(MUTED)),
        ]);
        lines.push(if i == input.selected {
            apply_selection(pad_line(line, inner.width))
        } else if hovered_row == Some(i) {
            pad_line(line, inner.width).style(Style::default().bg(hover_tint()))
        } else {
            line
        });
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_pairing(f: &mut Frame, area: Rect, lines: &[String], code: &str) {
    // Wide enough for the QR *and* the full code text (the manual-entry
    // fallback) — truncating the code would make it useless.
    let qr_width = lines.first().map(|l| l.chars().count()).unwrap_or(40) as u16;
    let width = (qr_width.max(code.chars().count() as u16) + 4).min(area.width);
    let height = (lines.len() as u16 + 4).min(area.height);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" scan with the Unpeel iPhone app · m to close ");
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    // Scanners need dark modules on light: render inverted (the glyphs are
    // chosen so a white foreground paints the quiet zone).
    let mut rendered: Vec<Line> = lines
        .iter()
        .map(|l| {
            Line::from(Span::styled(
                l.clone(),
                Style::default().fg(Color::White).bg(Color::Black),
            ))
        })
        .collect();
    rendered.push(Line::from(Span::styled(
        code.chars().take(inner.width as usize).collect::<String>(),
        Style::default().fg(Color::DarkGray),
    )));
    f.render_widget(Paragraph::new(rendered).centered(), inner);
}

/// Settings replaces the sidebar (desktop parity): sections on the left with
/// a back row on top, detail on the right.
fn draw_settings(
    f: &mut Frame,
    sidebar: Rect,
    detail: Rect,
    app: &App,
    section: usize,
    row: usize,
) {
    const SECTIONS: [&str; 6] = [
        "Presets", "Access", "Remote", "Projects", "About", "Cleanup",
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" settings ");
    let inner = block.inner(sidebar);
    f.render_widget(block, sidebar);
    let mut lines = vec![
        Line::from(Span::styled(
            "‹ back (esc)",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];
    for (i, name) in SECTIONS.iter().enumerate().filter(|(_, n)| !n.is_empty()) {
        let mut line = Line::from(Span::styled(
            format!("  {name}"),
            if i == section {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ));
        if i == section {
            line = apply_selection(pad_line(line, inner.width));
        }
        lines.push(line);
    }
    f.render_widget(Paragraph::new(lines), inner);

    // Remote draws its own structured detail (cards + footer) instead of
    // the generic single-paragraph pane.
    if section == 2 {
        draw_remote_settings(f, detail, app, row);
        return;
    }
    let (title, body) = settings_detail(app, section, row, detail.width.saturating_sub(2));
    let mut detail_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" {title} "));
    // Sections with an add action carry a clickable [+] on the right of
    // their title bar; `settings_add_hit` maps the click back.
    if matches!(section, 0 | 3) {
        detail_block = detail_block.title_top(
            Line::from(vec![
                Span::styled("[", Style::default().fg(MUTED)),
                Span::styled(
                    "+",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("] ", Style::default().fg(MUTED)),
            ])
            .right_aligned(),
        );
    }
    let detail_inner = detail_block.inner(detail);
    f.render_widget(detail_block, detail);
    f.render_widget(Paragraph::new(body), detail_inner);
}

/// Settings ▸ Remote as structured cards instead of one flat paragraph:
/// a bordered "controls this host" card (serving state + a paired-device
/// table with a column header), a bordered "unpeel link" card (license /
/// profile + relay enrollment), and a dim key-hint footer.
///
/// Desktop parity: SettingsView.swift's RemoteSettingsPanel — the inbound
/// "Controls This Mac" list, then the merged Unpeel Link section (the
/// standalone Link tab folded in here 2026-08-13). The app's outbound
/// "This App Controls" list has no TUI counterpart yet; a TUI is driven
/// outbound via `unpeel --host ssh://HOST`.
///
/// Keyboard row model is unchanged (devices, then the Link rows), and
/// `handle_settings_mouse` keeps its device-row screen offset in lockstep
/// with this card layout — device rows start on screen row 3 (card border,
/// serving line, table header).
fn draw_remote_settings(f: &mut Frame, detail: Rect, app: &App, row: usize) {
    let dim = Style::default().fg(MUTED);
    let devices = crate::paired_devices();
    let link_first = devices.len();
    let inner_width = detail.width.saturating_sub(2);
    let marker = |i: usize| if i == row { "▸ " } else { "  " };
    let card = |title: &'static str| {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Line::from(Span::styled(title, Style::default().fg(HEADER))))
    };

    // ── controls this host: serving state + the paired-device table ──
    let mut host_lines = vec![match &app.mobile_server {
        Some(server) => Line::from(vec![
            Span::styled(" ● ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("serving controllers on port {} (app closed)", server.port),
                Style::default().fg(Color::Green),
            ),
        ]),
        None => Line::from(vec![
            Span::styled(" ○ ", dim),
            Span::styled(
                "the Unpeel app is serving controllers — close it to serve here",
                dim,
            ),
        ]),
    }];
    if devices.is_empty() {
        host_lines.push(Line::from(Span::styled(
            "   no paired devices — share this host to pair one",
            dim,
        )));
    } else {
        host_lines.push(Line::from(Span::styled(
            format!(
                "   {:<19}{:<12}{:<22}{}",
                "device", "platform", "last seen", "access"
            ),
            Style::default().fg(Color::DarkGray),
        )));
        for (i, device) in devices.iter().enumerate() {
            let name: String = device.name.chars().take(18).collect();
            let mut line = Line::from(vec![
                Span::raw(format!("   {name:<19}")),
                Span::styled(format!("{:<12}", device_platform(device)), dim),
                Span::styled(format!("{:<22}", device_last_seen(device)), dim),
                if device.relay_allowed {
                    Span::styled("Link", Style::default().fg(Color::Green))
                } else {
                    Span::styled("direct only", Style::default().fg(Color::Yellow))
                },
            ]);
            if i == row {
                line = apply_selection(pad_line(line, inner_width));
            }
            host_lines.push(line);
        }
    }

    // ── unpeel link: license/profile rows, then relay enrollment ──
    let mut link_lines: Vec<Line> = Vec::new();
    match unpeel_core::license::stored() {
        Some((_, payload)) => {
            link_lines.push(Line::from(vec![
                Span::styled(" ● ", Style::default().fg(Color::Green)),
                Span::raw("active — "),
                Span::styled(
                    format!(
                        "licensed to {} · {} seat{}",
                        payload.email,
                        payload.seats,
                        if payload.seats == 1 { "" } else { "s" }
                    ),
                    dim,
                ),
            ]));
            link_lines.push(Line::from(""));
            link_lines.push(Line::from(format!(
                " {}deactivate this machine  (⏎)",
                marker(link_first)
            )));
            let name = crate::profile_value("profile_display_name");
            let name_shown = if !app.link_input.is_empty() && row == link_first + 1 {
                format!("{}▏", app.link_input)
            } else if name.is_empty() {
                "type a name, ⏎ to save".to_string()
            } else {
                name
            };
            link_lines.push(Line::from(vec![
                Span::raw(format!(" {}display name  ", marker(link_first + 1))),
                Span::styled(name_shown, dim),
            ]));
            let avatar = crate::profile_value("profile_avatar");
            link_lines.push(Line::from(vec![
                Span::raw(format!(" {}avatar        ", marker(link_first + 2))),
                Span::styled(
                    if avatar.is_empty() {
                        "none — ⏎ to pick".to_string()
                    } else {
                        format!("{avatar}  (⏎ to change)")
                    },
                    dim,
                ),
            ]));
            link_lines.push(Line::from(Span::styled(
                "   name and avatar identify you in Unpeel Apps",
                dim,
            )));
        }
        None => {
            link_lines.push(Line::from(vec![
                Span::styled(" free", Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(" — Unpeel Link connects your devices on any network", dim),
            ]));
            link_lines.push(Line::from(""));
            let draft = if app.link_input.is_empty() {
                Span::styled("paste license key, ⏎ to activate", dim)
            } else {
                Span::raw(format!("{}▏", app.link_input))
            };
            link_lines.push(Line::from(vec![
                Span::raw(format!(" {}key  ", marker(link_first))),
                draft,
            ]));
            link_lines.push(Line::from(Span::styled(
                "        get a key at unpeel.com/link",
                dim,
            )));
        }
    }
    link_lines.push(Line::from(""));
    // The enrollment list: which paired devices ride the relay. There is
    // no global toggle — the uplink runs while at least one device is
    // enrolled (and the entitlement is on disk). The TUI never had a
    // stored uplink preference, so there is nothing to migrate here; the
    // desktop's one-shot migration narrows the shared per-device flags
    // this list reads.
    let enrolled: Vec<&str> = devices
        .iter()
        .filter(|d| d.relay_allowed)
        .map(|d| d.name.as_str())
        .collect();
    link_lines.push(Line::from(vec![
        Span::styled(" on link  ", dim),
        if enrolled.is_empty() {
            Span::styled("no devices — every connection stays direct", dim)
        } else {
            Span::styled(enrolled.join(", "), Style::default().fg(Color::Green))
        },
    ]));
    link_lines.push(if app.relay_uplink.is_some() {
        Line::from(vec![
            Span::styled(" relay    ", dim),
            Span::styled("connected", Style::default().fg(Color::Green)),
        ])
    } else if app.mobile_server.is_some() {
        if unpeel_core::relay_uplink::cached_entitlement_for_host().is_some() {
            Line::from(vec![
                Span::styled(" relay    ", dim),
                Span::styled("starting…", dim),
            ])
        } else {
            Line::from(vec![
                Span::styled(" relay    ", dim),
                Span::styled("off — needs an Unpeel Link subscription", dim),
            ])
        }
    } else {
        Line::from(vec![
            Span::styled(" relay    ", dim),
            Span::styled("managed by the app while it runs", dim),
        ])
    });

    // ── layout: two cards, then the key-hint footer ──
    let [host_area, link_area, footer_area] = Layout::vertical([
        Constraint::Length(host_lines.len() as u16 + 2),
        Constraint::Length(link_lines.len() as u16 + 2),
        Constraint::Min(0),
    ])
    .areas(detail);
    let host_block = card(" controls this host ");
    f.render_widget(Paragraph::new(host_lines), host_block.inner(host_area));
    f.render_widget(host_block, host_area);
    let link_block = card(" unpeel link ");
    f.render_widget(Paragraph::new(link_lines), link_block.inner(link_area));
    f.render_widget(link_block, link_area);

    let footer = vec![
        Line::from(""),
        Line::from(vec![
            folder_badge("⏎/S"),
            Span::styled(" share this host (QR)   ", dim),
            folder_badge("L"),
            Span::styled(" add/remove device on Link   ", dim),
            folder_badge("x"),
            Span::styled(" unpair", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            " auth: per-device bearer token · code: one-time, 5 min · stored: SHA-256",
            dim,
        )),
        Line::from(Span::styled(" iPhone app beta: unpeel.com/ios", dim)),
    ];
    f.render_widget(Paragraph::new(footer), footer_area);
}

/// "iOS 0.1" — the platform column of a paired-device row
/// (RemoteSettingsPanel.deviceDetail's first half).
fn device_platform(device: &crate::PairedDevice) -> String {
    let version = device
        .app_version
        .as_deref()
        .map(|v| format!(" {v}"))
        .unwrap_or_default();
    format!("{}{}", device.platform, version)
}

/// "last seen 2h ago" — with a relative timestamp because a TUI row has no
/// hover (the desktop shows an absolute date).
fn device_last_seen(device: &crate::PairedDevice) -> String {
    match device.last_seen_unix_ms {
        None => "never seen".to_string(),
        Some(ms) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(ms);
            let secs = now.saturating_sub(ms) / 1000;
            let ago = match secs {
                0..=59 => "just now".to_string(),
                60..=3599 => format!("{}m ago", secs / 60),
                3600..=86_399 => format!("{}h ago", secs / 3600),
                _ => format!("{}d ago", secs / 86_400),
            };
            format!("last seen {ago}")
        }
    }
}

fn settings_detail(
    app: &App,
    section: usize,
    row: usize,
    width: u16,
) -> (String, Vec<Line<'static>>) {
    let dim = Style::default().fg(MUTED);
    match section {
        1 => {
            let mut lines = vec![Line::from(Span::styled(
                "policies apply live — the MCP host re-reads them per call",
                dim,
            ))];
            lines.push(Line::from(""));
            for (i, (key, label, cycle)) in crate::ACCESS_SETTINGS.iter().enumerate() {
                let value = crate::access_setting_value(key, cycle);
                let accent = match value.as_str() {
                    "off" | "deny" | "false" => Color::DarkGray,
                    "allow" | "on" | "true" => Color::Green,
                    _ => Color::Yellow,
                };
                let mut line = Line::from(vec![
                    Span::raw(format!("  {label:<38}")),
                    Span::styled(value, Style::default().fg(accent)),
                ]);
                if i == row {
                    line = apply_selection(pad_line(line, width));
                }
                lines.push(line);
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("⏎ cycle the selected policy", dim)));
            ("Access".into(), lines)
        }
        0 => {
            let presets = crate::app_state_presets();
            let mut lines = vec![
                Line::from(Span::styled("shared presets (app-state.json)", dim)),
                Line::from(""),
            ];
            for (i, (label, command, enabled, starred)) in presets.iter().enumerate() {
                let mut line = Line::from(vec![
                    Span::styled(
                        if *enabled { "  ✓ " } else { "  ✕ " },
                        Style::default().fg(if *enabled {
                            Color::Green
                        } else {
                            Color::DarkGray
                        }),
                    ),
                    Span::raw(if *starred { "⭑ " } else { "  " }),
                    Span::styled(
                        format!("{label:<26}"),
                        if *enabled {
                            Style::default().fg(cli_color(command))
                        } else {
                            dim
                        },
                    ),
                    Span::styled(command.clone(), dim),
                ]);
                if i == row {
                    line = apply_cli_selection(pad_line(line, width), command);
                }
                lines.push(line);
            }
            // The blank add row: always the last selectable row — select
            // it (↓, '+', or a click) and type, ⏎ commits. Mirrors the
            // desktop's "Add command" field at the bottom of its list.
            let placeholder = "Add command (e.g. claude --plan)";
            let mut add_line = if app.preset_add.is_empty() && row != presets.len() {
                Line::from(vec![
                    Span::styled("  ❯ ", Style::default().fg(Color::Cyan)),
                    Span::styled(placeholder, dim),
                ])
            } else {
                let mut spans = vec![
                    Span::styled("  ❯ ", Style::default().fg(Color::Cyan)),
                    Span::raw(app.preset_add.clone()),
                ];
                if row == presets.len() {
                    spans.push(Span::styled("▏", Style::default().fg(Color::Cyan)));
                    if app.preset_add.is_empty() {
                        spans.push(Span::styled(placeholder, dim));
                    } else {
                        spans.push(Span::styled("  ⏎ add", dim));
                    }
                }
                Line::from(spans)
            };
            if row == presets.len() {
                add_line = apply_selection(pad_line(add_line, width));
            }
            lines.push(add_line);
            // Un-migrated installs only: presets still held in the app's
            // UserDefaults overlay are read-only until the app runs once and
            // folds them into app-state.json (then this section is empty —
            // fallback_presets drops superseded overlay rows).
            for (label, command) in crate::sessions::fallback_presets(app.overlay.as_ref()) {
                if !presets.iter().any(|(_, c, _, _)| *c == command) {
                    lines.push(Line::from(vec![
                        Span::styled(format!("  · {label:<28}"), dim),
                        Span::styled(
                            format!("{command}  (in the app — open it once to migrate)"),
                            dim,
                        ),
                    ]));
                }
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "⏎ enable/disable   x remove   J/K reorder   * quick-launch",
                dim,
            )));
            lines.push(Line::from(Span::styled(
                "order picks each CLI's default — topmost enabled preset wins",
                dim,
            )));
            ("Presets".into(), lines)
        }
        3 => {
            let mut seen = std::collections::BTreeSet::new();
            for item in &app.model.items {
                if let SidebarItem::Header(name) = item {
                    seen.insert(name.clone());
                }
            }
            let mut lines: Vec<Line> = seen
                .into_iter()
                .map(|name| Line::from(format!("  {name}")))
                .collect();
            if lines.is_empty() {
                lines.push(Line::from(Span::styled("  no projects", dim)));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("+ add a project by path", dim)));
            ("Projects".into(), lines)
        }
        4 => {
            let home = unpeel_core::app_paths::unpeel_home();
            let lines = vec![
                Line::from(format!("  unpeel {}", env!("CARGO_PKG_VERSION"))),
                Line::from(vec![
                    Span::raw("  home    "),
                    Span::styled(home.display().to_string(), dim),
                ]),
                Line::from(vec![
                    Span::raw("  sidebar "),
                    Span::styled(
                        // Standalone is the TUI's native mode, not a fallback
                        // — it hosts sessions and reads shared state from
                        // disk with no app present. When the app IS running
                        // its sidebar is mirrored instead, so both UIs show
                        // identical rows (the app layers UserDefaults-only
                        // state the disk files don't carry yet).
                        if app.bridge_mode {
                            "synced with the app".to_string()
                        } else if app.feed_note.is_empty() || app.feed_note == "app offline" {
                            "standalone".to_string()
                        } else {
                            format!("standalone ({})", app.feed_note)
                        },
                        dim,
                    ),
                ]),
                Line::from(vec![
                    Span::raw("  hooks   "),
                    Span::styled(
                        app.hook_port
                            .map(|p| format!("listening on {p}"))
                            .unwrap_or_else(|| "unavailable".into()),
                        dim,
                    ),
                ]),
            ];
            ("About".into(), lines)
        }
        5 => {
            // Desktop parity: Settings ▸ Advanced ▸ Cleanup — the single
            // auto-stop-and-archive knob, shared via app-state.json.
            let minutes = crate::auto_stop_archive_minutes();
            let mut lines = vec![
                Line::from(Span::styled(
                    "idle sessions are stopped and archived — restore + Resume continues",
                    dim,
                )),
                Line::from(""),
            ];
            let mut line = Line::from(vec![
                Span::raw(format!(
                    "  {:<42}",
                    "Auto-stop and archive inactive terminals"
                )),
                Span::styled(
                    crate::auto_stop_archive_label(minutes),
                    Style::default().fg(if minutes == 0 {
                        Color::DarkGray
                    } else {
                        Color::Green
                    }),
                ),
            ]);
            if row == 0 {
                line = apply_selection(pad_line(line, width));
            }
            lines.push(line);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "pinned, selected, unread, working, and plain-shell sessions are left alone",
                dim,
            )));
            lines.push(Line::from(Span::styled("⏎ cycle the cutoff", dim)));
            ("Cleanup".into(), lines)
        }
        _ => ("Settings".into(), Vec::new()),
    }
}

/// First-run screen: what we found on this machine, and what we'd set up.
fn draw_first_run(f: &mut Frame, area: Rect, first_run: &crate::FirstRun) {
    let width = 74.min(area.width);
    let height = (first_run.presets.len().min(6) + first_run.projects.len() + 12)
        .min(area.height as usize) as u16;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    };
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(" welcome to unpeel ")
        .title_bottom(Line::from(Span::styled(
            " space toggles · ⏎ set up · esc skip ",
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines = vec![Line::from(Span::styled(
        " Presets for the CLIs installed here",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    if first_run.presets.is_empty() {
        lines.push(Line::from(Span::styled(
            "   none found on PATH — add one later with `unpeel presets add`",
            Style::default().fg(MUTED),
        )));
    }
    for preset in first_run.presets.iter().take(6) {
        lines.push(Line::from(vec![
            Span::styled("   ✓ ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("{:<22}", preset.label),
                Style::default().fg(cli_color(&preset.command)),
            ),
            Span::styled(preset.command.clone(), Style::default().fg(MUTED)),
        ]));
    }
    if first_run.presets.len() > 6 {
        lines.push(Line::from(Span::styled(
            format!("   +{} more", first_run.presets.len() - 6),
            Style::default().fg(MUTED),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Projects, from where your sessions have run",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    if first_run.projects.is_empty() {
        lines.push(Line::from(Span::styled(
            "   nothing to suggest yet — add one with + in the sidebar",
            Style::default().fg(MUTED),
        )));
    }
    for (i, project) in first_run.projects.iter().enumerate() {
        let ticked = first_run.accepted.get(i).copied().unwrap_or(false);
        let line = Line::from(vec![
            Span::styled(
                if ticked { "   [x] " } else { "   [ ] " },
                Style::default().fg(if ticked { Color::Green } else { MUTED }),
            ),
            Span::styled(format!("{:<20}", project.name), Style::default()),
            Span::styled(
                format!("{:<7}", if project.is_repo { "git" } else { "" }),
                Style::default().fg(MUTED),
            ),
            Span::styled(
                format!(
                    "{} session{}",
                    project.session_count,
                    if project.session_count == 1 { "" } else { "s" }
                ),
                Style::default().fg(MUTED),
            ),
        ]);
        lines.push(if i == first_run.row {
            apply_selection(line)
        } else {
            line
        });
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// Every floating layer, in z-order. Both the normal and the settings
/// layout call this — a dialog opened from settings has to render over it,
/// which an early return used to skip.
fn draw_overlays(f: &mut Frame, area: Rect, app: &App) {
    // One match over one enum: adding a modal without rendering it no
    // longer compiles, which is the whole reason Modal exists.
    match &app.modal {
        None => {}
        Some(Modal::Help) => draw_help(f, area),
        Some(Modal::FirstRun(first_run)) => draw_first_run(f, area, first_run),
        Some(Modal::Palette { query, selected }) => draw_recents(f, area, app, query, *selected),
        Some(Modal::Activity { selected }) => draw_activity_menu(f, area, app, *selected),
        Some(Modal::ProjectInput(input)) => draw_project_input(f, area, input, app.mouse_pos),
        Some(Modal::Rename(input)) => draw_rename_prompt(f, area, input),
        Some(Modal::GroupInput { buffer, .. }) => {
            draw_text_prompt(f, area, "new group", "⏎ create · esc cancel", buffer)
        }
        Some(Modal::GroupRename { buffer, .. }) => {
            draw_text_prompt(f, area, "rename group", "⏎ save · esc cancel", buffer)
        }
        Some(Modal::PresetPicker {
            presets,
            selected,
            target,
            anchor,
        }) => draw_preset_picker(f, area, presets, *selected, target, *anchor),
        Some(Modal::Pairing { lines, code }) => draw_pairing(f, area, lines, code),
        Some(Modal::Menu { selected }) => draw_menu(f, area, *selected),
        Some(Modal::LocalUrls { rows, selected }) => {
            draw_local_urls_menu(f, area, app, rows, *selected)
        }
        Some(Modal::Context(menu)) => draw_context_menu(f, area, menu),
    }
}

#[derive(Clone, Copy)]
enum ActivityMenuVisualRow {
    SessionTitle(usize),
    SessionProject(usize),
    Divider,
    Empty,
    Footer(usize),
}

impl ActivityMenuVisualRow {
    fn action(self) -> Option<usize> {
        match self {
            Self::SessionTitle(index) | Self::SessionProject(index) | Self::Footer(index) => {
                Some(index)
            }
            Self::Divider | Self::Empty => None,
        }
    }
}

fn activity_menu_visual_rows(entries: &[crate::ActivityMenuEntry]) -> Vec<ActivityMenuVisualRow> {
    let mut rows = Vec::new();
    let active = entries.iter().take_while(|entry| entry.working).count();
    if entries.is_empty() {
        rows.push(ActivityMenuVisualRow::Empty);
    } else {
        for index in 0..entries.len() {
            if index == active && active > 0 && active < entries.len() {
                rows.push(ActivityMenuVisualRow::Divider);
            }
            rows.push(ActivityMenuVisualRow::SessionTitle(index));
            rows.push(ActivityMenuVisualRow::SessionProject(index));
        }
    }
    rows.push(ActivityMenuVisualRow::Divider);
    rows.push(ActivityMenuVisualRow::Footer(entries.len()));
    rows
}

/// Activity dropdown geometry: centered under the sidebar's top-right
/// control, allowed to overlap the terminal pane like a native popover, and
/// clamped to both narrow terminals and short panes.
pub fn activity_menu_rect(
    area: Rect,
    sidebar_width: u16,
    entries: &[crate::ActivityMenuEntry],
) -> Rect {
    let widest = entries
        .iter()
        .map(|entry| {
            entry
                .title
                .chars()
                .count()
                .max(entry.project.chars().count())
                + unpeel_core::integrations::command_head(&entry.command)
                    .chars()
                    .count()
                + 8
        })
        .max()
        .unwrap_or(24) as u16;
    let width = widest.clamp(30, 56).min(area.width.max(1));
    let available_height = area.height.saturating_sub(1).max(1);
    let height = (activity_menu_visual_rows(entries).len() as u16 + 2)
        .min(available_height)
        .max(1);
    let anchor = area.x + sidebar_width.saturating_sub(3).min(area.width);
    let x = anchor
        .saturating_sub(width / 2)
        .clamp(area.x, area.x + area.width.saturating_sub(width));
    Rect {
        x,
        y: if area.height > 1 { area.y + 1 } else { area.y },
        width,
        height,
    }
}

fn activity_menu_visible_start(
    rows: &[ActivityMenuVisualRow],
    selected: usize,
    visible: usize,
) -> usize {
    if rows.len() <= visible || visible == 0 {
        return 0;
    }
    let selected_row = rows
        .iter()
        .position(|row| row.action() == Some(selected))
        .unwrap_or(0);
    selected_row
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible))
}

/// Action under a click in the activity popup. Both lines of a session row
/// select the same action; divider/empty rows deliberately return None.
pub fn activity_menu_row_at(
    area: Rect,
    app: &App,
    selected: usize,
    col: u16,
    row: u16,
) -> Option<usize> {
    let entries = app.activity_menu_entries();
    let rows = activity_menu_visual_rows(&entries);
    let rect = activity_menu_rect(area, app.sidebar_width, &entries);
    let inner_height = rect.height.saturating_sub(2) as usize;
    let start = activity_menu_visible_start(&rows, selected, inner_height);
    if col <= rect.x
        || col >= rect.x + rect.width.saturating_sub(1)
        || row <= rect.y
        || row >= rect.y + rect.height.saturating_sub(1)
    {
        return None;
    }
    rows.get(start + (row - rect.y - 1) as usize)
        .and_then(|visual| visual.action())
}

pub fn activity_menu_frame_hit(area: Rect, app: &App, col: u16, row: u16) -> bool {
    let entries = app.activity_menu_entries();
    let rect = activity_menu_rect(area, app.sidebar_width, &entries);
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

fn draw_activity_menu(f: &mut Frame, area: Rect, app: &App, selected: usize) {
    let entries = app.activity_menu_entries();
    let visual_rows = activity_menu_visual_rows(&entries);
    let rect = activity_menu_rect(area, app.sidebar_width, &entries);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(Line::from(Span::styled(
            " recent activity ",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .title_bottom(Line::from(Span::styled(
            " ⏎ open · esc ",
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let visible = inner.height as usize;
    let start = activity_menu_visible_start(&visual_rows, selected, visible);
    let lines = visual_rows
        .iter()
        .skip(start)
        .take(visible)
        .map(|visual| match *visual {
            ActivityMenuVisualRow::SessionTitle(index) => {
                let entry = &entries[index];
                let provider = unpeel_core::integrations::command_head(&entry.command);
                let marker = if entry.working {
                    Span::styled(
                        spinner_frame().to_string(),
                        Style::default().fg(cli_color(&entry.command)),
                    )
                } else {
                    Span::styled("●", Style::default().fg(Color::Rgb(64, 140, 255)))
                };
                let title_max = (inner.width as usize)
                    .saturating_sub(provider.chars().count() + 6)
                    .max(1);
                let title = ellipsize(&entry.title, title_max);
                let used = title.chars().count() + provider.chars().count() + 5;
                let pad = (inner.width as usize).saturating_sub(used);
                let line = Line::from(vec![
                    Span::raw(" "),
                    marker,
                    Span::raw(" "),
                    Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(
                        provider.to_string(),
                        Style::default().fg(cli_color(&entry.command)),
                    ),
                    Span::raw(" "),
                ]);
                if selected == index {
                    apply_cli_selection(pad_line(line, inner.width), &entry.command)
                } else {
                    line
                }
            }
            ActivityMenuVisualRow::SessionProject(index) => {
                let entry = &entries[index];
                let project = ellipsize(&entry.project, inner.width.saturating_sub(5) as usize);
                let line = Line::from(vec![
                    Span::raw("   "),
                    Span::styled(project, Style::default().fg(MUTED)),
                ]);
                if selected == index {
                    apply_cli_selection(pad_line(line, inner.width), &entry.command)
                } else {
                    line
                }
            }
            ActivityMenuVisualRow::Divider => Line::from(Span::styled(
                "─".repeat(inner.width as usize),
                Style::default().fg(Color::DarkGray),
            )),
            ActivityMenuVisualRow::Empty => Line::from(Span::styled(
                "  No active sessions",
                Style::default().fg(MUTED),
            )),
            ActivityMenuVisualRow::Footer(index) => action_row(
                selected == index,
                false,
                inner.width,
                vec![Span::styled("  All recent", Style::default().fg(MUTED))],
                "›",
            ),
        })
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Geometry of a project context menu: anchored at the right-click, clamped
/// so it never overflows the screen (shifts left/up instead). Shared by the
/// renderer and the mouse hit-tests so they can never disagree.
pub fn context_menu_rect(area: Rect, menu: &crate::ContextMenu) -> Rect {
    let widest = menu
        .items
        .iter()
        .map(|(label, _)| label.chars().count())
        .max()
        .unwrap_or(8) as u16;
    let title_width = menu.title.chars().count() as u16 + 2;
    let width = (widest + 6)
        .max(title_width + 4)
        .max(16)
        .min(area.width.max(1));
    let height = (menu.items.len() as u16 + 2).min(area.height.max(1));
    let x = menu.anchor.0.min(area.x + area.width.saturating_sub(width));
    let y = menu
        .anchor
        .1
        .min(area.y + area.height.saturating_sub(height));
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// The context-menu row under the mouse, if any.
pub fn context_menu_row_at(
    area: Rect,
    menu: &crate::ContextMenu,
    col: u16,
    row: u16,
) -> Option<usize> {
    let rect = context_menu_rect(area, menu);
    let top = rect.y + 1;
    let count = menu.items.len() as u16;
    if col > rect.x
        && col < rect.x + rect.width.saturating_sub(1)
        && row >= top
        && row < top + count
    {
        Some((row - top) as usize)
    } else {
        None
    }
}

/// True when the mouse is anywhere on the context menu's frame.
pub fn context_menu_frame_hit(area: Rect, menu: &crate::ContextMenu, col: u16, row: u16) -> bool {
    let rect = context_menu_rect(area, menu);
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// A context menu: a floating popup at the right-click, titled with the
/// project or session it acts on — the desktop's menus, the herdr shape.
/// Color rows carry a swatch in the palette color they set.
fn draw_context_menu(f: &mut Frame, area: Rect, menu: &crate::ContextMenu) {
    let rect = context_menu_rect(area, menu);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(Line::from(Span::styled(
            format!(" {} ", menu.title),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = menu
        .items
        .iter()
        .enumerate()
        .map(|(i, (label, action))| {
            let on = i == menu.selected;
            let marker = if on { "› " } else { "  " };
            let text_style = if on {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut spans = vec![Span::styled(marker.to_string(), text_style)];
            if let crate::CtxAction::SetColor(raw) = action {
                let swatch_style = raw
                    .and_then(project_folder_color)
                    .map(|c| Style::default().fg(c))
                    .unwrap_or_else(|| Style::default().fg(MUTED));
                spans.push(Span::styled("● ", swatch_style));
            }
            spans.push(Span::styled(label.clone(), text_style));
            let line = Line::from(spans);
            if on {
                let mut line = apply_selection(pad_line(line, inner.width));
                // The selection bar repaints every span; put the swatch's
                // color back so the highlighted row still shows its hue.
                if let crate::CtxAction::SetColor(Some(raw)) = action {
                    if let (Some(color), Some(span)) =
                        (project_folder_color(raw), line.spans.get_mut(1))
                    {
                        span.style = span.style.fg(color);
                    }
                }
                line
            } else {
                line
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Geometry of the footer menu popup — anchored to the sidebar's bottom-left,
/// just above the "menu" label it opens from. Shared by the renderer and the
/// mouse hit-test so they can never disagree.
pub fn menu_rect(area: Rect) -> Rect {
    let items = crate::MENU_ITEMS;
    let widest = items
        .iter()
        .map(|item| item.label.chars().count() + item.shortcut.chars().count())
        .max()
        .unwrap_or(8) as u16;
    // Marker + a flexible gap + shortcut + borders, but never narrower than
    // the frame's titles.
    let width = (widest + 9).min(area.width.saturating_sub(2)).max(16);
    let height = (items.len() as u16 + 2).min(area.height);
    let x = area.x + 1;
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height + 1))
        .max(area.y);
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// The menu row under a click, if any (the outside-the-frame case dismisses).
pub fn menu_row_at(area: Rect, col: u16, row: u16) -> Option<usize> {
    let rect = menu_rect(area);
    let top = rect.y + 1;
    let count = crate::MENU_ITEMS.len() as u16;
    if col > rect.x
        && col < rect.x + rect.width.saturating_sub(1)
        && row >= top
        && row < top + count
    {
        Some((row - top) as usize)
    } else {
        None
    }
}

/// True when a click lands anywhere on the menu popup's frame.
pub fn menu_frame_hit(area: Rect, col: u16, row: u16) -> bool {
    let rect = menu_rect(area);
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// The footer menu popup: anchored to the sidebar's bottom-left, above the
/// "menu" label it opens from — the herdr shape, one signpost list.
fn draw_menu(f: &mut Frame, area: Rect, selected: usize) {
    let items = crate::MENU_ITEMS;
    let rect = menu_rect(area);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(Line::from(Span::styled(
            " menu ",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .title_bottom(Line::from(Span::styled(
            " ⏎ open · esc ",
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let on = i == selected;
            let marker = if on { "› " } else { "  " };
            action_row(
                on,
                false,
                inner.width,
                vec![Span::styled(
                    format!("{marker}{}", item.label),
                    if on {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )],
                item.shortcut,
            )
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// Label of the preview top-right chip advertising the project's live local
/// sites. Icon-only — the full URLs live in the dropdown; a lone URL opens
/// directly on click, several show the ▾.
pub fn local_urls_chip_label(urls: &[String]) -> Option<String> {
    urls.first()?;
    // 🌐 matches the desktop titlebar's globe. Emoji-wide — the chip rect
    // measures display cells, so hit-testing stays exact.
    Some(if urls.len() == 1 {
        " 🌐 ".to_string()
    } else {
        format!(" 🌐 {} ▾ ", urls.len())
    })
}

/// Where the chip paints on the preview's top border row — shared by the
/// renderer and the mouse hit-test so they can never disagree. Width is
/// display cells, not chars: the link glyph is emoji-wide.
pub fn local_urls_chip_rect(preview: Rect, urls: &[String]) -> Option<Rect> {
    let label = local_urls_chip_label(urls)?;
    let width = Line::from(label.as_str()).width() as u16;
    if preview.height == 0 || preview.width < width + 2 {
        return None;
    }
    Some(Rect {
        x: preview.x + preview.width - 1 - width,
        y: preview.y,
        width,
        height: 1,
    })
}

/// The local-sites dropdown frame: anchored under the chip on the preview's
/// top-right, clamped to the screen. Shared by rendering and hit-testing.
pub fn local_urls_menu_rect(area: Rect, sidebar_width: u16, rows: &[crate::LocalUrlRow]) -> Rect {
    let widest = rows
        .iter()
        .map(|r| r.label().chars().count())
        .max()
        .unwrap_or(10) as u16;
    let width = (widest + 4).max(16).min(area.width.max(1));
    let height = (rows.len() as u16 + 2).min(area.height.max(1));
    let x = (area.x + area.width)
        .saturating_sub(width + 1)
        .max(sidebar_width.min(area.x + area.width.saturating_sub(width)));
    let y = (area.y + 1).min(area.y + area.height.saturating_sub(height));
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// The dropdown row under the mouse, if any.
pub fn local_urls_row_at(
    area: Rect,
    sidebar_width: u16,
    rows: &[crate::LocalUrlRow],
    col: u16,
    row: u16,
) -> Option<usize> {
    let rect = local_urls_menu_rect(area, sidebar_width, rows);
    let top = rect.y + 1;
    let count = rows.len() as u16;
    if col > rect.x
        && col < rect.x + rect.width.saturating_sub(1)
        && row >= top
        && row < top + count
    {
        Some((row - top) as usize)
    } else {
        None
    }
}

/// True when the mouse is anywhere on the dropdown's frame.
pub fn local_urls_frame_hit(
    area: Rect,
    sidebar_width: u16,
    rows: &[crate::LocalUrlRow],
    col: u16,
    row: u16,
) -> bool {
    let rect = local_urls_menu_rect(area, sidebar_width, rows);
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

fn draw_local_urls_menu(
    f: &mut Frame,
    area: Rect,
    app: &App,
    rows: &[crate::LocalUrlRow],
    selected: usize,
) {
    let rect = local_urls_menu_rect(area, app.sidebar_width, rows);
    f.render_widget(ratatui::widgets::Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(FOCUS))
        .title(Line::from(Span::styled(
            " local sites ",
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .title_bottom(Line::from(Span::styled(
            " ⏎ run · esc ",
            Style::default().fg(MUTED),
        )));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let on = i == selected;
            let marker = if on { "› " } else { "  " };
            let stop_row = matches!(row, crate::LocalUrlRow::Stop { .. });
            let mut line = Line::from(vec![Span::styled(
                format!("{marker}{}", row.label()),
                match (on, stop_row) {
                    (true, _) => Style::default().add_modifier(Modifier::BOLD),
                    (false, true) => Style::default().fg(Color::Red),
                    (false, false) => Style::default(),
                },
            )]);
            if on {
                line = apply_selection(pad_line(line, inner.width));
            }
            line
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

pub fn draw(f: &mut Frame, app: &App, snapshots: &SnapshotService) {
    // No permanent bottom row. A message OVERLAYS the last line rather than
    // taking layout space: giving it a row only when it exists would resize
    // the session's PTY every time an outcome appeared, and taking a row
    // permanently is what left a blank gap under the terminal.
    let area = f.area();
    let rows = [
        area,
        Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1.min(area.height),
        },
    ];
    let columns = Layout::horizontal([Constraint::Length(app.sidebar_width), Constraint::Min(10)])
        .split(rows[0]);
    if let Some((section, row)) = app.settings {
        draw_settings(f, columns[0], columns[1], app, section, row);
        draw_status_bar(f, rows[1], app);
        draw_overlays(f, rows[0], app);
        draw_toast(f, rows[0], app);
        return;
    }
    draw_sidebar(f, columns[0], app);
    draw_preview(f, columns[1], app, snapshots);
    draw_status_bar(f, rows[1], app);
    draw_overlays(f, rows[0], app);
    draw_toast(f, rows[0], app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use unpeel_core::terminal_viewport::{TerminalViewportRow, TerminalViewportStyleRun};

    #[test]
    fn fold_toggle_sits_clear_of_the_menu_label() {
        // At the narrowest sidebar the two bottom-border hit zones must not
        // overlap: menu bottom-left, fold-all bottom-right.
        let divider = MIN_SIDEBAR_WIDTH - 1;
        for col in 0..divider {
            assert!(
                !(menu_label_hit(col) && fold_label_hit(col, divider)),
                "col {col} hits both labels"
            );
        }
        // The label's own cells hit (a right-aligned title ends at the cell
        // before the corner), a cell of slack on the left, and cells further
        // in don't.
        assert!(fold_label_hit(divider - 1, divider));
        assert!(fold_label_hit(divider - 4, divider));
        assert!(!fold_label_hit(divider - 5, divider));
    }

    #[test]
    fn footer_menu_ends_with_exit() {
        let exit = crate::MENU_ITEMS.last().expect("footer menu has rows");
        assert_eq!(exit.label, "Exit");
        assert_eq!(exit.shortcut, "q");
        assert_eq!(exit.action, crate::MenuAction::Exit);

        let area = Rect::new(0, 0, 80, 24);
        let rect = menu_rect(area);
        let exit_row = rect.y + crate::MENU_ITEMS.len() as u16;
        assert_eq!(
            menu_row_at(area, rect.x + 2, exit_row),
            Some(crate::MENU_ITEMS.len() - 1)
        );
    }

    #[test]
    fn activity_control_owns_only_the_sidebar_top_right() {
        let divider = MIN_SIDEBAR_WIDTH - 1;
        assert!(activity_button_hit(divider - 1, divider));
        assert!(activity_button_hit(divider - 5, divider));
        assert!(!activity_button_hit(divider - 6, divider));
        assert!(!activity_button_hit(divider, divider));
        // Even at the minimum width, the Projects title has a quiet gap
        // before the activity target begins.
        assert!(" Projects ".chars().count() as u16 <= divider - 6);
    }

    #[test]
    fn activity_popup_stays_anchored_and_clamped() {
        let entries = vec![crate::ActivityMenuEntry {
            session_id: "s1".into(),
            title: "A useful recent session".into(),
            project: "unpeel".into(),
            command: "codex".into(),
            working: true,
            unread: false,
        }];
        let roomy = Rect::new(0, 0, 120, 30);
        let rect = activity_menu_rect(roomy, 32, &entries);
        assert_eq!(rect.y, 1, "popover drops below the top-border control");
        assert!(rect.x < 32 && rect.x + rect.width > 29);
        assert!(rect.x + rect.width <= roomy.width);

        let narrow = Rect::new(0, 0, 24, 5);
        let rect = activity_menu_rect(narrow, 20, &entries);
        assert!(rect.x + rect.width <= narrow.width);
        assert!(rect.y + rect.height <= narrow.height);
    }

    #[test]
    fn age_follows_activity_not_creation() {
        let created = 1_754_300_000_000; // days ago
        let active_now = 1_754_999_000_000;
        assert_eq!(row_age_ms(active_now, created), active_now);
        // Never-run sessions still show their creation age.
        assert_eq!(row_age_ms(0, created), created);
        // Clock skew can't make a row look older than it is.
        assert_eq!(row_age_ms(created - 5_000, created), created);
    }

    #[test]
    fn attention_matches_the_desktop_orange() {
        assert_eq!(status_color(Status::Attention), Color::Rgb(245, 158, 11));
    }

    #[test]
    fn codex_accent_matches_the_desktop() {
        assert_eq!(cli_color("codex --full-auto"), Color::Rgb(194, 146, 254));
    }

    #[test]
    fn folder_count_is_parenthesized_and_date_muted() {
        let count = folder_count(3);
        assert_eq!(count.content, "(3)");
        assert_eq!(count.style.fg, Some(Color::DarkGray));
        assert_eq!(count.style.bg, None);

        let action = right_flush_folder_badge(HEADER_ADD_LABEL);
        assert_eq!(action.content, " + New");
        assert!(
            action.style.bg.is_some(),
            "action should retain its button fill"
        );
    }

    #[test]
    fn shimmer_lights_a_travelling_band() {
        let spans = shimmer_spans("unpeel-design", Color::Rgb(196, 202, 216));
        assert_eq!(spans.len(), "unpeel-design".chars().count());
        let brightness: Vec<u8> = spans
            .iter()
            .map(|s| match s.style.fg {
                Some(Color::Rgb(r, _, _)) => r,
                _ => 0,
            })
            .collect();
        let max = *brightness.iter().max().unwrap();
        let min = *brightness.iter().min().unwrap();
        // A band: some cells lit, others not — never a uniform pulse.
        assert!(max > min, "expected a gradient across the label");
        assert!(min >= 150, "stayed in range: {min}..{max}");
        assert_eq!(shimmer_spans("x", Color::Cyan).len(), 1);
    }

    fn run(start: u16, len: u16, bg: Option<&str>) -> TerminalViewportStyleRun {
        TerminalViewportStyleRun {
            start,
            len,
            fg: None,
            bg: bg.map(str::to_owned),
            bold: false,
            inverse: false,
        }
    }

    #[test]
    fn trimmed_rows_keep_their_background_to_the_edge() {
        // What the host actually sends: text trimmed to the last glyph, a
        // style run still covering the full grid width.
        let row = TerminalViewportRow {
            text: "hi".into(),
            styles: vec![run(0, 10, Some("rgb:20,30,40"))],
            wrapped: false,
        };
        let line = row_to_line(&row, 10, None);
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, 10, "row should be padded to the grid width");
        let painted = line
            .spans
            .iter()
            .all(|s| s.style.bg == Some(Color::Rgb(20, 30, 40)));
        assert!(painted, "padded cells must keep the row background");
    }

    #[test]
    fn unstyled_padding_stays_transparent() {
        let row = TerminalViewportRow {
            text: "hi".into(),
            styles: vec![run(0, 2, Some("rgb:20,30,40"))],
            wrapped: false,
        };
        let line = row_to_line(&row, 6, None);
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, 6);
        let tail = line.spans.last().expect("a trailing span");
        assert_eq!(
            tail.style.bg, None,
            "cells the app never painted stay clear"
        );
    }

    #[test]
    fn wide_rows_are_not_truncated() {
        let row = TerminalViewportRow {
            text: "abcdefgh".into(),
            styles: vec![],
            wrapped: false,
        };
        let line = row_to_line(&row, 4, None);
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, 8);
    }

    #[test]
    fn terminal_styles_are_indexed_by_cells_after_wide_glyphs() {
        let row = TerminalViewportRow {
            text: "🙂x".into(),
            // The emoji occupies cells 0 and 1; x begins at cell 2.
            styles: vec![run(2, 1, Some("rgb:20,30,40"))],
            wrapped: false,
        };
        let line = row_to_line(&row, 3, None);
        let x = line
            .spans
            .iter()
            .find(|span| span.content.contains('x'))
            .expect("x span");
        assert_eq!(x.style.bg, Some(Color::Rgb(20, 30, 40)));
    }

    #[test]
    fn preset_picker_is_centered_without_a_mouse_anchor() {
        let area = Rect::new(0, 0, 120, 30);
        let presets = vec![
            ("Terminal".into(), String::new()),
            ("cat".into(), "cat".into()),
        ];
        let rect = preset_picker_rect(area, &presets, "unpeel · /tmp", None);
        assert_eq!(rect.width, 60);
        assert_eq!(rect.x, 30);
        assert_eq!(rect.y, 8);
    }

    #[test]
    fn preset_picker_drops_below_a_clicked_plus() {
        let area = Rect::new(0, 0, 120, 30);
        let presets = vec![
            ("Terminal".into(), String::new()),
            ("cat".into(), "cat".into()),
        ];
        let anchor = Some((38, 2));
        let rect = preset_picker_rect(area, &presets, "unpeel · /tmp", anchor);
        assert_eq!((rect.x, rect.y), (38, 2));
        assert_eq!(
            preset_picker_row_at(
                area,
                &presets,
                "unpeel · /tmp",
                0,
                anchor,
                rect.x + 1,
                rect.y + 1,
            ),
            Some(0),
        );
    }

    #[test]
    fn long_rename_values_grow_and_wrap_the_dialog() {
        let area = Rect::new(0, 0, 120, 30);
        let input = crate::RenameInput::new("s1".into(), "x".repeat(75));
        let rect = rename_prompt_rect(area, &input);
        let text = rename_text_rect(area, &input);
        assert_eq!(rect.height, 6, "two text rows plus dialog chrome");
        assert_eq!(
            rename_text_index_at(area, &input, text.x + 5, text.y + 1),
            Some(65),
            "the second visual row maps back into the original string",
        );
    }

    #[test]
    fn rename_selection_deletes_whole_unicode_characters() {
        let mut input = crate::RenameInput::new("s1".into(), "a🪴bc".into());
        input.begin_mouse_selection(1);
        input.finish_mouse_selection(3);
        assert_eq!(input.selection(), Some((1, 3)));
        input.backspace();
        assert_eq!(input.buffer, "ac");
        assert_eq!(input.cursor, 1);
        assert_eq!(input.selection(), None);
    }
}
