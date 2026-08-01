# Unified utility tab strip design

Date: 2026-08-01
Status: shipped; the titlebar-control contract below is superseded by
`2026-08-01-utility-panel-toggle-design.md` (the Changes button became the
utility-panel toggle).

## Goal

Keep the full-height right utility column while replacing its duplicated pane
header plus Terminal tab bar with one shared tab strip. Terminal sessions and
Changes appear as sibling tabs, matching the supplied references.

## Layout

The existing sidebar, conversation, and full-height right utility column remain
unchanged. The utility column owns one 40px top strip and one active body.

The strip may contain:

- `Terminal 1 ×`, `Terminal 2 ×`, and further live terminal sessions.
- One `Changes ×` tab.
- A trailing `+` button.

There is no separate `Terminal ×` or `Changes ×` header above this strip.

## State

Each chat keeps a session-scoped utility-tab state:

- Whether the full-height column is visible.
- Which utility tab is active.
- Whether Changes is open.

Terminal tab identity, PTY ownership, ordering, replay, and lifecycle remain in
`TerminalPanel`. The shell composes those terminal tab descriptors with the
optional Changes descriptor into one strip.

Changes remains in the strip when a Terminal tab is selected. Selecting a tab
only swaps the body; it does not collapse or resize the column.

## Interaction

- `+` opens a small menu containing only Terminal and Changes.
- Terminal creates and selects a new terminal tab.
- Changes opens or selects the existing Changes tab.
- Closing the active tab selects its next neighbor, then its previous neighbor.
- Closing the final tab collapses the utility column.
- The existing Terminal and Changes buttons remain visible in the conversation
  titlebar and open/select their corresponding tabs.
- `Cmd+J` continues toggling the utility column.
- The left-edge resize target, persisted width, and 200ms width transition stay
  unchanged.

## Scope

No Browser, Files, or Side Chat tabs. No changes to engine, RPC, PTY transport,
diff watching, or the panel's lateral placement.

## Verification

- Unit tests for tab ordering, active-neighbor selection, Changes persistence,
  and final-tab collapse.
- `cargo test -p comet-ui` and `cargo build -p comet`.
- Desktop smoke test covering two Terminal tabs plus Changes in one row, `+`
  menu actions, close fallback, keyboard toggle, resize, and session switching.
- Visual capture confirming a single top strip with no duplicated header.
