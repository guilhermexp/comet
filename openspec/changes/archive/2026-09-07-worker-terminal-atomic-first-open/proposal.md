## Why

The first opening of a Worker terminal can expose the historical replay as scrolling and partially drawn TUI frames. Reopening an already loaded terminal works; the first load should present its completed viewport in the same way.

## What Changes

- Keep the first viewport unavailable to painting until the recorded opening output boundary has been consumed.
- Remove time/chunk limits that reveal partially loaded history; transport errors remain visible and retry without publishing a partial viewport.
- Drain initial history without live-output waits or hidden-panel polling delays, preserving scrollback and subsequent live output.

## Capabilities

### New Capabilities

- `worker-terminal-initial-presentation`: Atomic first presentation of an existing Worker's terminal history.

### Modified Capabilities

None.

## Impact

`crates/ui/src/workers/terminal.rs` and its owning DOX. Existing terminal emulator, host protocol, Worker lifecycle and retained-view navigation remain in use. No dependencies or provider-specific behavior.
