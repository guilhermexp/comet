# Change: Archive the Worker session on screen with a keystroke

## Why

Closing a finished Worker takes the mouse today: right-click the sidebar row, then `Archive` (or `Stop and archive`). The Orchestrator has had a keyboard verb for the same intent since the shortcut inventory (`ArchiveSession`, `⇧⌘A`), but it only ever archives a Chat, so the Workers sidebar has no keyboard path at all. A finished session therefore sits in the tree until it is dismissed by hand, which is exactly the moment the user wants it gone.

## What Changes

- Handle AppKit's `CloseWindow` action (⌘W) in the Workers workspace by archiving the session the viewer is showing, and stop propagation only when something was archived so ⌘W still closes the window otherwise.
- Move the viewer to the neighbouring session that stays in the tree, and clear it when the project has none left.
- Leave `ArchiveSession` (`⇧⌘A`, Orchestrator Chats) untouched, as a separate action, so neither combo reaches the other sidebar's verb.

## Capabilities

### New Capabilities

- `workers-session-archive-shortcut`: keyboard archiving of the selected Worker session and the viewer handoff that follows it.

### Modified Capabilities

None.

## Impact

- `crates/ui/src/shell.rs`: the mode-gated `CloseWindow` handler and `worker_archive_shortcut_enabled`.
- `crates/ui/src/workers/model.rs`: `archive_selected_session`, reusing `stop_and_archive` and `selection_after_remove`.
- `⌘W` stops reaching a TUI running inside a Worker terminal; the app owns the combo now.
- No change to the archive endpoint, to the context menu, or to the customizable shortcut inventory.
