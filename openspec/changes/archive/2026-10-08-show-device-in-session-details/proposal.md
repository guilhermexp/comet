# Proposal

## Why

The Session card in Details shows the project but not the machine hosting its execution. Users already see device names in the sidebar and need the same identity while inspecting Details.

## What Changes

- Add a read-only Device row beside the project information in the existing Session card.
- Resolve the context's host through the shared device registry; refresh naturally with context and registry changes.
- Keep long device names contained and show an unknown-device placeholder when metadata is unavailable.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `details-sidebar`: Session card shows the device associated with its selected context.

## Impact

Only native desktop Details rendering and its owner documentation change. Existing DetailsContext and AppState device-name lookup provide identity; no engine, RPC, durable schema or dependency changes.
