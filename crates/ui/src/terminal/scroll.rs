use gpui::{Pixels, ScrollDelta, TouchPhase};

pub const SCROLLBAR_TRACK_INSET: f32 = 4.0;
pub const SCROLLBAR_HIT_WIDTH: f32 = 10.0;
pub const SCROLLBAR_THUMB_WIDTH: f32 = 3.0;
pub const SCROLLBAR_HOVER_THUMB_WIDTH: f32 = 4.5;
const SCROLLBAR_MIN_THUMB: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarMetrics {
    pub track_top: f32,
    pub track_height: f32,
    pub thumb_top: f32,
    pub thumb_height: f32,
    pub history_lines: usize,
}

impl ScrollbarMetrics {
    pub fn travel(self) -> f32 {
        (self.track_height - self.thumb_height).max(0.0)
    }

    pub fn offset_for_pointer(self, pointer_y: Pixels, grab_offset: f32) -> usize {
        let thumb_top =
            (f32::from(pointer_y) - self.track_top - grab_offset).clamp(0.0, self.travel());
        if self.travel() <= 0.0 {
            0
        } else {
            ((1.0 - thumb_top / self.travel()) * self.history_lines as f32).round() as usize
        }
    }
}

pub fn scrollbar_metrics(
    bounds: gpui::Bounds<Pixels>,
    rows: usize,
    history_lines: usize,
    display_offset: usize,
) -> Option<ScrollbarMetrics> {
    if history_lines == 0 {
        return None;
    }
    let track_height = (f32::from(bounds.size.height) - SCROLLBAR_TRACK_INSET * 2.0).max(0.0);
    if track_height <= 0.0 {
        return None;
    }
    let total_lines = history_lines.saturating_add(rows).max(1);
    let thumb_height = (track_height * rows as f32 / total_lines as f32)
        .max(SCROLLBAR_MIN_THUMB)
        .min(track_height);
    let travel = (track_height - thumb_height).max(0.0);
    let offset = display_offset.min(history_lines);
    let progress_from_top = 1.0 - offset as f32 / history_lines as f32;
    Some(ScrollbarMetrics {
        track_top: f32::from(bounds.top()) + SCROLLBAR_TRACK_INSET,
        track_height,
        thumb_top: travel * progress_from_top,
        thumb_height,
        history_lines,
    })
}

/// Retains sub-line wheel movement for one terminal scroll gesture.
///
/// macOS trackpads deliver precise pixel deltas that are commonly smaller
/// than one terminal row. Converting each event independently loses those
/// deltas, so keep the fractional remainder until it crosses a row boundary.
#[derive(Debug, Default)]
pub struct TerminalScrollGesture {
    residual_y: f32,
}

impl TerminalScrollGesture {
    pub fn steps(&mut self, delta: ScrollDelta, phase: TouchPhase, line_height: Pixels) -> i32 {
        match phase {
            TouchPhase::Started => {
                self.residual_y = 0.0;
                return 0;
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.residual_y = 0.0;
                return 0;
            }
            TouchPhase::Moved => {}
        }

        let delta_y = f32::from(delta.pixel_delta(line_height).y);
        if delta_y == 0.0 {
            return 0;
        }
        if self.residual_y != 0.0 && self.residual_y.signum() != delta_y.signum() {
            self.residual_y = 0.0;
        }
        self.residual_y += delta_y;

        let line_height = f32::from(line_height).max(f32::EPSILON);
        let steps = (self.residual_y / line_height).trunc() as i32;
        self.residual_y -= steps as f32 * line_height;
        steps
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MouseProtocol {
    Normal,
    Utf8,
    #[default]
    Sgr,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalScrollModes {
    pub mouse_reporting: bool,
    pub mouse_protocol: MouseProtocol,
    pub alternate_screen: bool,
    pub mouse_alternate_scroll: bool,
    pub application_cursor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalScrollAction {
    Write(Vec<u8>),
    Scrollback,
}

pub fn terminal_scroll_action(
    modes: TerminalScrollModes,
    steps: i32,
    column: usize,
    row: usize,
) -> TerminalScrollAction {
    let up = steps > 0;
    let count = steps.unsigned_abs() as usize;
    if modes.mouse_reporting {
        let button = if up { 64 } else { 65 };
        let report = match modes.mouse_protocol {
            MouseProtocol::Sgr => format!(
                "\x1b[<{button};{};{}M",
                column.saturating_add(1),
                row.saturating_add(1)
            )
            .into_bytes(),
            MouseProtocol::Normal => normal_mouse_report(button, column, row, false),
            MouseProtocol::Utf8 => normal_mouse_report(button, column, row, true),
        };
        return TerminalScrollAction::Write(report.repeat(count));
    }
    if modes.alternate_screen && modes.mouse_alternate_scroll {
        let sequence = match (up, modes.application_cursor) {
            (true, true) => "\x1bOA",
            (false, true) => "\x1bOB",
            (true, false) => "\x1b[A",
            (false, false) => "\x1b[B",
        };
        return TerminalScrollAction::Write(sequence.repeat(count).into_bytes());
    }
    TerminalScrollAction::Scrollback
}

fn normal_mouse_report(button: u8, column: usize, row: usize, utf8: bool) -> Vec<u8> {
    let max_point = if utf8 { 2015 } else { 223 };
    if column >= max_point || row >= max_point {
        return Vec::new();
    }
    let mut report = vec![0x1b, b'[', b'M', 32 + button];
    let mut push_position = |position: usize| {
        let encoded = 33 + position;
        if utf8 && position >= 95 {
            report.push((0xC0 + encoded / 64) as u8);
            report.push((0x80 + (encoded & 63)) as u8);
        } else {
            report.push(encoded as u8);
        }
    };
    push_position(column);
    push_position(row);
    report
}

#[cfg(test)]
mod tests {
    use gpui::{ScrollDelta, TouchPhase, point, px};

    use super::{
        MouseProtocol, TerminalScrollAction, TerminalScrollGesture, TerminalScrollModes,
        terminal_scroll_action,
    };

    #[test]
    fn precise_sub_line_deltas_accumulate_across_one_gesture() {
        let mut gesture = TerminalScrollGesture::default();
        let delta = ScrollDelta::Pixels(point(px(0.0), px(5.0)));

        assert_eq!(gesture.steps(delta, TouchPhase::Started, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 1);
    }

    #[test]
    fn a_new_gesture_discards_the_previous_fraction() {
        let mut gesture = TerminalScrollGesture::default();
        let delta = ScrollDelta::Pixels(point(px(0.0), px(10.0)));

        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Started, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 0);
    }

    #[test]
    fn direction_change_replaces_the_previous_fraction() {
        let mut gesture = TerminalScrollGesture::default();

        assert_eq!(
            gesture.steps(
                ScrollDelta::Pixels(point(px(0.0), px(10.0))),
                TouchPhase::Moved,
                px(18.0),
            ),
            0
        );
        assert_eq!(
            gesture.steps(
                ScrollDelta::Pixels(point(px(0.0), px(-10.0))),
                TouchPhase::Moved,
                px(18.0),
            ),
            0
        );
        assert_eq!(
            gesture.steps(
                ScrollDelta::Pixels(point(px(0.0), px(-10.0))),
                TouchPhase::Moved,
                px(18.0),
            ),
            -1
        );
    }

    #[test]
    fn line_wheel_steps_are_immediate_and_terminal_phases_emit_nothing() {
        let mut gesture = TerminalScrollGesture::default();
        let delta = ScrollDelta::Lines(point(0.0, 3.0));

        assert_eq!(gesture.steps(delta, TouchPhase::Moved, px(18.0)), 3);
        assert_eq!(gesture.steps(delta, TouchPhase::Ended, px(18.0)), 0);
        assert_eq!(gesture.steps(delta, TouchPhase::Cancelled, px(18.0)), 0);
    }

    #[test]
    fn captured_wheel_uses_the_negotiated_mouse_protocol() {
        let modes = |mouse_protocol| TerminalScrollModes {
            mouse_reporting: true,
            mouse_protocol,
            ..TerminalScrollModes::default()
        };

        assert_eq!(
            terminal_scroll_action(modes(MouseProtocol::Sgr), 1, 0, 0),
            TerminalScrollAction::Write(b"\x1b[<64;1;1M".to_vec())
        );
        assert_eq!(
            terminal_scroll_action(modes(MouseProtocol::Normal), 1, 0, 0),
            TerminalScrollAction::Write(vec![0x1b, b'[', b'M', 96, 33, 33])
        );
        assert_eq!(
            terminal_scroll_action(modes(MouseProtocol::Utf8), 1, 100, 0),
            TerminalScrollAction::Write(vec![0x1b, b'[', b'M', 96, 0xC2, 0x85, 33])
        );
    }
}
