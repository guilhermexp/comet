# Lateral terminal design

Date: 2026-08-01
Status: approved

## Problem

The integrated terminal is only discoverable through `Cmd+J` and currently opens below the conversation. Users expect a visible title-bar control and a terminal beside the chat so both remain usable at once.

## Goals

- Add a terminal icon button to the right side of the session title bar.
- Open the terminal in the existing resizable right-side card.
- Keep the chat and terminal visible side by side.
- Preserve `Cmd+J`, terminal tabs, PTY processes, and per-session panel state.
- Make Terminal and Changes mutually exclusive uses of the same right-side space.

## Non-goals

- No terminal or RPC protocol changes.
- No second simultaneous right-side panel.
- No terminal for a new-session canvas without a selected chat.
- No redesign of the terminal emulator or Changes content.

## Interaction

The title bar shows a terminal glyph beside the existing Changes button. Both buttons use the current 28 px title-bar control style and display a selected background when active.

Clicking the terminal button or pressing `Cmd+J`:

- opens Terminal in the right-side card when closed;
- switches the card from Changes to Terminal when Changes is active;
- closes the card when Terminal is already active;
- focuses the active terminal when it opens.

Clicking Changes or pressing `Cmd+B` follows the symmetric behavior. Only one utility pane can be active.

The lateral card uses the existing right-pane width, resize handle, 200 ms width transition, rounded border, and outer gutters. The Terminal content retains its own header, close control, terminal tabs, add-tab control, and PTY viewport.

On the new-session canvas, the terminal button remains visible for discoverability. Opening it displays the existing instruction to select a chat. Selecting a chat while the panel is open creates or restores that chat's terminal tab.

## State model

Replace the two independent `terminal_open` and `changes_open` booleans with one optional per-session utility-pane value:

- `None`
- `Terminal`
- `Changes`

The state remains in memory and keyed by chat. The new-session canvas remains keyed by space. This representation enforces mutual exclusion instead of repairing invalid combinations during rendering.

The right-pane width remains global and persisted. The obsolete bottom-terminal height, height tween, and vertical resize state are removed.

## Component changes

### Session title bar

Add the terminal button before Changes. Extend the title-bar button primitive with an active presentation shared by both controls.

### Shell

Use the active utility-pane value to determine right-pane visibility, content, animation, focus, and message-rail width. Remove the bottom terminal container from the main conversation column.

### Right-side card

Render either `TerminalPanel` or `Changes` inside the current card. Terminal creation remains lazy. Changes still starts its diff watch only when selected and available for a Git-backed space.

### Terminal panel

No PTY behavior changes. Its existing close action continues dispatching `ToggleTerminal`, which now closes the right-side card.

## Error handling

Existing terminal errors remain inside the terminal panel. A missing selected chat renders the existing `Select a chat to open a terminal` state. Changes remains unavailable for spaces without Git; the terminal control remains available.

## Verification

- Unit tests prove utility-pane default state, per-session isolation, toggle close, and Terminal/Changes mutual exclusion.
- Existing terminal panel tests continue to pass.
- Build the desktop app.
- Launch the development demo and select a real session.
- Click the terminal title-bar button and verify the terminal opens beside the chat, receives focus, accepts input, resizes horizontally, closes, and restores.
- Verify `Cmd+J`, Changes switching, session switching, and the new-session empty state.
