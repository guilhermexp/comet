//! The family look: shared colors, the busy spinner, the shimmer, and the
//! footer hint idiom. Extracted from `unpeel-tui`'s `ui.rs` so plugins and
//! the TUI read as one product.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Muted-but-legible secondary text. `DarkGray` maps to the theme's
/// brightest-black, which on Ghostty's default is a hair above the
/// background — fine for hairlines, unreadable for words.
pub const MUTED: Color = Color::Rgb(150, 156, 170);

/// Headers: structure, so a shade under body text but nowhere near
/// near-invisible grey.
pub const HEADER: Color = Color::Rgb(196, 202, 216);

/// The focus/selection accent. An explicit RGB rather than palette `Cyan`,
/// which most terminal themes render as a green-teal — this reads as the
/// intended purple-gray everywhere.
pub const FOCUS: Color = Color::Rgb(156, 147, 184);

/// The desktop's blue unread dot.
pub const UNREAD: Color = Color::Rgb(64, 140, 255);

/// Attention (needs the user) accent.
pub const ATTENTION: Color = Color::LightRed;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The braille busy spinner, keyed off wall time so every widget animates
/// in phase.
pub fn spinner_frame() -> &'static str {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    SPINNER_FRAMES[(millis / 100) as usize % SPINNER_FRAMES.len()]
}

/// A highlight that TRAVELS across the label (skeleton-loader style)
/// rather than pulsing its brightness — pulsing the whole word reads as
/// blinking. Each character is lit by its distance from a band sweeping
/// left to right.
pub fn shimmer_spans(text: &str, base: Color) -> Vec<Span<'static>> {
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

/// The footer hint idiom: `key` in header color, its label muted, pairs
/// separated by a middle dot — `a add · space toggle · q quit`.
pub fn hint_line(hints: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(MUTED)));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(HEADER).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default().fg(MUTED),
        ));
    }
    Line::from(spans)
}
