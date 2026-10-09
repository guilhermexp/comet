# Proposal

## Why

Workers Details exposes token totals but omits how long ago a Worker was dispatched. Users need the same compact age convention already shown in the Chat sidebar.

## What Changes

- Display time since creation beside the token total, or beside the command when telemetry is missing.
- Retain that origin across heartbeat, output and terminal-state updates.
- Refresh the visible label with the existing lightweight Details clock; keep layout bounded and show a clear launch-time tooltip.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workers-widget-interaction`: Readable Worker dispatch age beside metadata.

## Impact

Pure Worker projection and native Details render, reusing the existing sidebar formatter and timestamps. No provider, engine, protocol or CLI Worker lifecycle changes. Background execution support remains in the sibling `improve-workers-background-observability` change.
