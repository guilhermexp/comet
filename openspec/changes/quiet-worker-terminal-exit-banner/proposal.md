# Change: Stop reporting a finished Worker as a terminal disconnect

## Why

The Unpeel host answers every request made against a dead session with `409: session has exited` — write, resize and poll alike. The Worker terminal renders any of those failures as a red `Worker terminal disconnected: …` banner over the grid, so the normal end of a Worker looked like a transport error. The footer of the same surface already states that the session ended, which makes the banner a second, alarming report of a fact the user can already read.

## What Changes

- Classify `session has exited` as an expected end state rather than a transport failure and keep it out of the disconnect banner.
- Keep every other failure (host down, refused socket, malformed payload, non-409 status) on the banner.

## Capabilities

### New Capabilities

- `workers-terminal-diagnostics`: which Worker terminal failures reach the user as a disconnect banner.

### Modified Capabilities

None.

## Impact

- `crates/ui/src/workers/terminal.rs`: one classifier plus the render-time filter that both the poll error and the resize error pass through.
- No change to the resize retry ceiling, to logging, or to the host contract.
