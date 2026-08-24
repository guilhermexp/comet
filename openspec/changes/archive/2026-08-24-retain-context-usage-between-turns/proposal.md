# Change: Retain context usage between turns

## Why

Starting a new harness process clears the selected chat's last context-window
measurement, so the composer incorrectly returns to “Aguardando primeiro
turno” until the new turn finishes.

## What Changes

- Preserve the last per-chat context snapshot during turn startup.
- Replace it only when the runtime reports a newer snapshot.

## Capabilities

### New Capabilities

- `context-usage-continuity`: last-known per-chat context usage presentation.

## Impact

- `crates/engine/src/sessions.rs`
- `crates/engine/AGENTS.md`
