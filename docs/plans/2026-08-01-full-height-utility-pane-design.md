# Full-height utility pane design

Date: 2026-08-01
Status: approved

## Goal

Make the shared Terminal/Changes pane a true full-height right column. It starts at the top window edge and reaches the right and bottom edges, matching the supplied reference instead of appearing as a rounded inset card below the global titlebar.

## Layout

The ready-state shell becomes three visual columns: sidebar, conversation, and the optional utility pane. The conversation keeps its existing inset card. The utility pane leaves that card system and owns its full-height column.

The main titlebar ends at the utility-pane seam. When open, the utility pane renders its own top header inside the full-height column:

- `Terminal ×` when the terminal is active.
- `Changes ×` when the diff viewer is active.

Only the thin vertical seam and resize target between the conversation and utility pane remain. Remove the utility pane's outer radius, outer border card, top/right/bottom gutters, and the horizontal separation created by placing it below the global titlebar.

## Content

Terminal keeps its existing session-scoped PTY state and internal tab bar (`Terminal 1`, `+`, reorder, close) directly below the new pane header. Changes renders its current content directly below the same header contract.

Terminal and Changes remain mutually exclusive. Switching between them replaces the header label and body without collapsing the column. Closing the active pane collapses the column with the existing 200 ms width transition.

## Interaction

- The existing Terminal and Changes titlebar buttons remain visible in the conversation titlebar.
- The pane header `×` closes the active pane.
- `Cmd+J` continues toggling Terminal.
- The Changes shortcut retains its current behavior.
- The left-edge drag target continues resizing the shared width.
- The persisted width and session-scoped active pane remain unchanged.

## Scope

No changes to engine, RPC, PTY ownership, replay, input, diff watching, or persistence schema. The work is a shell composition and chrome change.

## Verification

- Unit tests for pane selection and close behavior.
- `cargo test -p comet-ui` and `cargo build -p comet`.
- Desktop smoke test covering Terminal/Changes switching, header close, keyboard toggle, resize, session switching, and focus.
- Visual capture confirming the pane reaches the top/right/bottom edges and has no rounded outer card or horizontal divider.
