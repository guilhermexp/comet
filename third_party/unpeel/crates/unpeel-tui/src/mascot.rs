//! One-shot startup animation for the Unpeel mascot.
//!
//! The source art lives in the sibling `unpeel-mascot` project. Keep these
//! three poses aligned with `mascot-animated.sh`: the seated monkey never
//! moves internally; only its tail bends in and unfurls.

use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;
use ratatui::Frame;

const RISE: Duration = Duration::from_millis(600);
const WAVE: Duration = Duration::from_millis(1_200);
const FALL: Duration = Duration::from_millis(600);
const FRAME_TIME_MS: u128 = 150;

const HEIGHT: i32 = 13;
const WIDTH: i32 = 26;
const MIN_TERMINAL_WIDTH: u16 = WIDTH as u16 + 2;
const MIN_TERMINAL_HEIGHT: u16 = HEIGHT as u16 + 1;

// Cropped to the occupied columns of the 17-column source frames. Each
// source pixel becomes two terminal cells, preserving the mascot's aspect.
const UP: [&str; HEIGHT as usize] = [
    "...DDDDD.....",
    "..DDDDDDD....",
    ".MLLLLLLLM...",
    "LMLBLLLBLML..",
    "LMLBLLLBLML..",
    ".MLLLLLLLM..M",
    "..MMLLLMM...M",
    "...MMMMM....M",
    "...MMMMMM...M",
    "..MMMMMMMM..M",
    "..MMMMMMMMMM.",
    "..MMMMMMMMM..",
    "..LLMMMLLM...",
];

const LEAN: [&str; HEIGHT as usize] = [
    "...DDDDD.....",
    "..DDDDDDD....",
    ".MLLLLLLLM...",
    "LMLBLLLBLML..",
    "LMLBLLLBLML..",
    ".MLLLLLLLM...",
    "..MMLLLMM..MM",
    "...MMMMM....M",
    "...MMMMMM...M",
    "..MMMMMMMM..M",
    "..MMMMMMMMMM.",
    "..MMMMMMMMM..",
    "..LLMMMLLM...",
];

const CURL: [&str; HEIGHT as usize] = [
    "...DDDDD.....",
    "..DDDDDDD....",
    ".MLLLLLLLM...",
    "LMLBLLLBLML..",
    "LMLBLLLBLML..",
    ".MLLLLLLLM...",
    "..MMLLLMM.MM.",
    "...MMMMM..M.M",
    "...MMMMMM...M",
    "..MMMMMMMM..M",
    "..MMMMMMMMMM.",
    "..MMMMMMMMM..",
    "..LLMMMLLM...",
];

const STOPS: [(u8, u8, u8); 6] = [
    (217, 119, 87),
    (0, 196, 196),
    (67, 194, 81),
    (79, 168, 255),
    (76, 125, 247),
    (155, 97, 234),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pose {
    Up,
    Lean,
    Curl,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Sample {
    pose: Pose,
    /// How far the complete mascot has moved below its seated position.
    y_offset: i32,
}

pub struct StartupMascot {
    started: Instant,
    enabled: bool,
}

impl Default for StartupMascot {
    fn default() -> Self {
        Self::new()
    }
}

impl StartupMascot {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            // PTY conformance cases assert on the app underneath this purely
            // decorative layer. Unit tests below cover the animation itself.
            enabled: std::env::var("UNPEEL_TEST").as_deref() != Ok("1")
                && std::env::var("TERM").as_deref() != Ok("dumb"),
        }
    }

    pub fn draw(&self, frame: &mut Frame) {
        if !self.enabled {
            return;
        }
        let area = frame.area();
        if !fits(area) {
            return;
        }
        let Some(sample) = sample_at(self.started.elapsed()) else {
            return;
        };
        frame.render_widget(MascotWidget { sample }, area);
    }
}

fn fits(area: Rect) -> bool {
    area.width >= MIN_TERMINAL_WIDTH && area.height >= MIN_TERMINAL_HEIGHT
}

fn sample_at(elapsed: Duration) -> Option<Sample> {
    if elapsed < RISE {
        let progress = elapsed.as_secs_f32() / RISE.as_secs_f32();
        return Some(Sample {
            pose: Pose::Up,
            y_offset: ((1.0 - ease_out_cubic(progress)) * HEIGHT as f32).round() as i32,
        });
    }
    if elapsed < RISE + WAVE {
        let wave_elapsed = elapsed.saturating_sub(RISE).as_millis();
        let pose = match (wave_elapsed / FRAME_TIME_MS) % 4 {
            0 => Pose::Up,
            1 | 3 => Pose::Lean,
            _ => Pose::Curl,
        };
        return Some(Sample { pose, y_offset: 0 });
    }
    if elapsed < RISE + WAVE + FALL {
        let fall_elapsed = elapsed.saturating_sub(RISE + WAVE);
        let progress = fall_elapsed.as_secs_f32() / FALL.as_secs_f32();
        return Some(Sample {
            pose: Pose::Up,
            y_offset: (ease_in_cubic(progress) * HEIGHT as f32).round() as i32,
        });
    }
    None
}

fn ease_out_cubic(value: f32) -> f32 {
    1.0 - (1.0 - value).powi(3)
}

fn ease_in_cubic(value: f32) -> f32 {
    value.powi(3)
}

struct MascotWidget {
    sample: Sample,
}

impl Widget for MascotWidget {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let right = i32::from(area.x) + i32::from(area.width);
        let bottom = i32::from(area.y) + i32::from(area.height);
        let left = right - WIDTH - 1;
        let top = bottom - HEIGHT + self.sample.y_offset;
        let frame = match self.sample.pose {
            Pose::Up => &UP,
            Pose::Lean => &LEAN,
            Pose::Curl => &CURL,
        };

        for (row_index, row) in frame.iter().enumerate() {
            let y = top + row_index as i32;
            if y < i32::from(area.y) || y >= bottom {
                continue;
            }
            for (column, pixel) in row.bytes().enumerate() {
                if pixel == b'.' {
                    continue;
                }
                let x = left + column as i32 * 2;
                let color = pixel_color(pixel, column);
                for cell_x in [x, x + 1] {
                    if cell_x >= i32::from(area.x) && cell_x < right {
                        let cell = &mut buffer[(cell_x as u16, y as u16)];
                        cell.reset();
                        cell.set_symbol("█").set_fg(color);
                    }
                }
            }
        }
    }
}

fn pixel_color(pixel: u8, column: usize) -> Color {
    if pixel == b'B' {
        return Color::Rgb(0, 0, 0);
    }
    let (r, g, b) = gradient(column);
    if pixel == b'L' {
        Color::Rgb(r + (255 - r) / 2, g + (255 - g) / 2, b + (255 - b) / 2)
    } else {
        Color::Rgb(r, g, b)
    }
}

fn gradient(column: usize) -> (u8, u8, u8) {
    // Source columns 1...11 become cropped columns 0...10. Columns beyond
    // the final stop are the purple tail.
    let position = column.min(10) as f32 / 10.0 * (STOPS.len() - 1) as f32;
    let lower = (position.floor() as usize).min(STOPS.len() - 2);
    let mix = if position >= (STOPS.len() - 1) as f32 {
        1.0
    } else {
        position - lower as f32
    };
    let from = STOPS[lower];
    let to = STOPS[lower + 1];
    let channel = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * mix).round() as u8;
    (
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rises_waves_and_falls_once() {
        assert!(sample_at(Duration::ZERO).unwrap().y_offset >= HEIGHT - 1);
        assert_eq!(sample_at(RISE).unwrap().y_offset, 0);
        assert_eq!(
            sample_at(RISE + Duration::from_millis(150)).unwrap().pose,
            Pose::Lean
        );
        assert_eq!(
            sample_at(RISE + Duration::from_millis(300)).unwrap().pose,
            Pose::Curl
        );
        assert!(
            sample_at(RISE + WAVE + Duration::from_millis(500))
                .unwrap()
                .y_offset
                > 0
        );
        assert!(sample_at(RISE + WAVE + FALL).is_none());
    }

    #[test]
    fn tail_is_purple_and_face_is_lighter_than_body() {
        assert_eq!(gradient(12), STOPS[STOPS.len() - 1]);
        let Color::Rgb(body_r, body_g, body_b) = pixel_color(b'M', 3) else {
            panic!("body must be RGB");
        };
        let Color::Rgb(face_r, face_g, face_b) = pixel_color(b'L', 3) else {
            panic!("face must be RGB");
        };
        assert!(face_r >= body_r && face_g >= body_g && face_b >= body_b);
    }

    #[test]
    fn paints_only_mascot_cells_at_the_bottom_right() {
        let area = Rect::new(0, 0, 40, 20);
        let mut buffer = Buffer::empty(area);
        // The first source row begins with three transparent pixels. Prove
        // the overlay leaves the app beneath them untouched.
        buffer[(13, 7)].set_symbol("x");
        MascotWidget {
            sample: Sample {
                pose: Pose::Up,
                y_offset: 0,
            },
        }
        .render(area, &mut buffer);
        assert_eq!(buffer[(13, 7)].symbol(), "x");
        assert_eq!(buffer[(19, 7)].symbol(), "█");
        assert_eq!(buffer[(39, 19)].symbol(), " ");
    }

    #[test]
    fn stays_out_of_terminals_that_cannot_fit_the_art() {
        assert!(fits(Rect::new(
            0,
            0,
            MIN_TERMINAL_WIDTH,
            MIN_TERMINAL_HEIGHT
        )));
        assert!(!fits(Rect::new(
            0,
            0,
            MIN_TERMINAL_WIDTH - 1,
            MIN_TERMINAL_HEIGHT
        )));
        assert!(!fits(Rect::new(
            0,
            0,
            MIN_TERMINAL_WIDTH,
            MIN_TERMINAL_HEIGHT - 1
        )));
    }
}
