# Design: Default-open turn-step tool groups

## Context

The transcript already has two independent disclosures: `TurnSteps` owns the
completed assistant prefix and each nested `ToolGroup` owns its individual
cards. When a child settles into `TurnSteps`, the projection currently clears
both the group and card-detail auto-open flags.

## Goals

- Match the supplied reference by showing every nested tool card.
- Keep command output, invocation bodies, and diffs compact by default.
- Preserve manual fold overrides and virtualized remount behavior.

## Non-goals

- Changing the outer `TurnSteps` disclosure default.
- Auto-opening tool-card details.
- Changing top-level settled groups, row identity, caching, or height math.

## Decision

During `settle_turn_steps_child`, set `ToolGroup.auto_open = true` and keep
`detail_auto_open = false`. The existing renderer computes the effective state
as `FoldState.open.unwrap_or(auto_open)`, so an explicit click remains
authoritative without introducing new state or invalidation paths.

## Risks

Long turns consume more vertical space when their outer disclosure is open.
That is intentional and remains bounded by the existing block virtualizer;
individual groups and card details can still be collapsed.
