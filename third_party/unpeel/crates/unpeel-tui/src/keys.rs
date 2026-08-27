//! Focused-terminal key translation: crossterm `KeyEvent` → the bytes a real
//! terminal would send, forwarded verbatim to the session PTY.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn key_event_to_string(key: &KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let mut out = String::new();
    if alt {
        out.push('\x1b');
    }
    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                match c.to_ascii_lowercase() {
                    lc @ 'a'..='z' => out.push((lc as u8 - b'a' + 1) as char),
                    '[' => out.push('\x1b'),
                    '\\' => out.push('\x1c'),
                    ']' => out.push('\x1d'),
                    '^' => out.push('\x1e'),
                    '_' => out.push('\x1f'),
                    ' ' => out.push('\0'),
                    other => out.push(other),
                }
            } else {
                out.push(c);
            }
        }
        KeyCode::Enter => out.push('\r'),
        KeyCode::Tab => out.push('\t'),
        KeyCode::BackTab => out.push_str("\x1b[Z"),
        KeyCode::Backspace => out.push('\x7f'),
        KeyCode::Esc => out.push('\x1b'),
        KeyCode::Up | KeyCode::Down | KeyCode::Right | KeyCode::Left => {
            let dir = match key.code {
                KeyCode::Up => 'A',
                KeyCode::Down => 'B',
                KeyCode::Right => 'C',
                _ => 'D',
            };
            if ctrl {
                out.push_str(&format!("\x1b[1;5{dir}"));
            } else if shift {
                out.push_str(&format!("\x1b[1;2{dir}"));
            } else {
                out.push_str(&format!("\x1b[{dir}"));
            }
        }
        KeyCode::Home => out.push_str("\x1b[H"),
        KeyCode::End => out.push_str("\x1b[F"),
        KeyCode::PageUp => out.push_str("\x1b[5~"),
        KeyCode::PageDown => out.push_str("\x1b[6~"),
        KeyCode::Delete => out.push_str("\x1b[3~"),
        KeyCode::Insert => out.push_str("\x1b[2~"),
        KeyCode::F(n) => out.push_str(match n {
            1 => "\x1bOP",
            2 => "\x1bOQ",
            3 => "\x1bOR",
            4 => "\x1bOS",
            5 => "\x1b[15~",
            6 => "\x1b[17~",
            7 => "\x1b[18~",
            8 => "\x1b[19~",
            9 => "\x1b[20~",
            10 => "\x1b[21~",
            11 => "\x1b[23~",
            12 => "\x1b[24~",
            _ => return None,
        }),
        _ => return None,
    }
    Some(out)
}
