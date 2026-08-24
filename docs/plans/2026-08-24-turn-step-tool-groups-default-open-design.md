# Turn-Step Tool Groups Default-Open Design

## Goal

Match the approved Orchestrator.dev transcript reference: when `TurnSteps` is
expanded, every nested tool group immediately shows its individual Run, Read,
Edit, and other tool cards instead of presenting only collapsed summary rows.

## Decision

Keep the existing two disclosure levels and change only the nested group
default. A tool group moved into `TurnSteps` receives `auto_open = true`, while
`detail_auto_open` remains false. The group therefore exposes the cards shown
in the reference without automatically dumping command output, invocation
bodies, or diffs.

Existing `FoldState.open` remains authoritative after a click. Users may still
collapse an individual group, and that choice survives virtualized remounts.
Top-level settled tool groups outside `TurnSteps`, streaming defaults, stable
row ids, analytic heights, caches, sticky-user geometry, and file-card inner
scrolling remain unchanged.

## Verification

- A focused row-projection test fails with the old closed default and passes
  when completed-prefix groups show cards.
- The same test verifies that per-card details remain closed.
- The complete `zeron-ui` suite and workspace checks guard virtualization and
  transcript regressions.
- A real GPUI smoke compares the expanded transcript against the supplied
  third screenshot.
