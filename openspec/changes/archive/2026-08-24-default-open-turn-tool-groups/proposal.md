# Change: Show turn-step tool cards by default

## Why

Expanded assistant turn steps currently leave every nested tool group closed,
so long turns show only repeated `Ran`/`Read` summaries. The approved
Orchestrator.dev reference exposes the individual tool cards immediately while
keeping each card's output or diff independently collapsible.

## Decisions

- **D-01:** Tool groups projected inside `TurnSteps` default to open.
- **D-02:** Per-card invocation, output, and diff details remain closed.
- **D-03:** Explicit user fold state continues to override the default.
- **D-04:** Top-level settled groups and streaming behavior remain unchanged.

## What Changes

- Set the nested group default during `TurnSteps` child settlement.
- Add a deterministic projection regression and headed GPUI smoke.
- Document the durable transcript contract in UI DOX.

## Capabilities

### New Capabilities

- `turn-step-tool-groups`: default visibility and disclosure behavior for tool
  cards inside assistant turn steps.

## Impact

- `crates/ui/src/transcript.rs`: completed-prefix group projection and tests.
- `crates/ui/AGENTS.md`: transcript disclosure contract.
