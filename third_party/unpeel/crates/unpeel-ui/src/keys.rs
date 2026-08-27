//! Keybinding conventions shared by the family: vim-style j/k plus arrows,
//! g/G for the ends, Enter to act, Esc to back out, q or Ctrl-C to quit.
//!
//! Callers apply this only while a list has focus — while the user is
//! typing into an input, feed characters to the input instead.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::portable::{ListDirection, ReorderCommand};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    Up,
    Down,
    Top,
    Bottom,
    Select,
    Back,
    Quit,
}

/// Translate a key event to the family's list-navigation vocabulary.
pub fn nav(key: &KeyEvent) -> Option<Nav> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    Some(match key.code {
        KeyCode::Char('c') if ctrl => Nav::Quit,
        KeyCode::Char('q') => Nav::Quit,
        KeyCode::Char('j') | KeyCode::Down => Nav::Down,
        KeyCode::Char('k') | KeyCode::Up => Nav::Up,
        KeyCode::Char('g') | KeyCode::Home => Nav::Top,
        KeyCode::Char('G') | KeyCode::End => Nav::Bottom,
        KeyCode::Enter => Nav::Select,
        KeyCode::Esc => Nav::Back,
        _ => return None,
    })
}

/// Map keys for a top-to-bottom vertical collection to the shared reorder
/// helper. Use [`reorder_list`] when a portable List may render bottom-to-top.
///
/// Space or Enter picks up/drops the selected item, Escape cancels, and
/// Up/Down or k/j moves the selection or grabbed item.
pub fn reorder_vertical(key: &KeyEvent) -> Option<ReorderCommand> {
    Some(match key.code {
        KeyCode::Char('k') | KeyCode::Up => ReorderCommand::Previous,
        KeyCode::Char('j') | KeyCode::Down => ReorderCommand::Next,
        KeyCode::Char(' ') | KeyCode::Enter => ReorderCommand::ToggleGrab,
        KeyCode::Esc => ReorderCommand::Cancel,
        _ => return None,
    })
}

/// Map spatial list keys to logical reorder commands for either List direction.
pub fn reorder_list(key: &KeyEvent, direction: ListDirection) -> Option<ReorderCommand> {
    let command = reorder_vertical(key)?;
    Some(match (direction, command) {
        (ListDirection::BottomToTop, ReorderCommand::Previous) => ReorderCommand::Next,
        (ListDirection::BottomToTop, ReorderCommand::Next) => ReorderCommand::Previous,
        _ => command,
    })
}

/// Map keys for a horizontally ordered collection to the shared reorder
/// helper. Useful for tabs and table columns.
pub fn reorder_horizontal(key: &KeyEvent) -> Option<ReorderCommand> {
    Some(match key.code {
        KeyCode::Char('h') | KeyCode::Left => ReorderCommand::Previous,
        KeyCode::Char('l') | KeyCode::Right => ReorderCommand::Next,
        KeyCode::Char(' ') | KeyCode::Enter => ReorderCommand::ToggleGrab,
        KeyCode::Esc => ReorderCommand::Cancel,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorder_keys_are_axis_specific_and_accessible() {
        assert_eq!(
            reorder_vertical(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(ReorderCommand::Next)
        );
        assert_eq!(
            reorder_horizontal(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Some(ReorderCommand::Previous)
        );
        assert_eq!(
            reorder_vertical(&KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(ReorderCommand::ToggleGrab)
        );
        assert_eq!(
            reorder_horizontal(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(ReorderCommand::Cancel)
        );
        assert_eq!(
            reorder_vertical(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            reorder_list(
                &KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
                ListDirection::BottomToTop,
            ),
            Some(ReorderCommand::Next)
        );
    }
}
