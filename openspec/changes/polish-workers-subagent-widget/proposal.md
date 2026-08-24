# Change: Polish Workers subagent widget

## Why

The first activity can open without user intent, active subagents no longer show
their lifecycle spinner, and Details To-dos rows drift from the inline card.

## What Changes

- Start every workflow and subagent disclosure collapsed.
- Restore the existing lifecycle status renderer to subagent rows.
- Match Details To-dos row geometry to the inline To-do card.

## Capabilities

### New Capabilities

- `workers-widget-interaction`: predictable disclosure and lifecycle feedback.

### Modified Capabilities

- `todo-status-alignment`: shared inline/widget row geometry.

## Impact

- `crates/ui/src/details_sidebar/{widgets,view,todos}.rs`
- `crates/ui/AGENTS.md`
