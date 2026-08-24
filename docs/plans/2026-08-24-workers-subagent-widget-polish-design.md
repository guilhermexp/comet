# Workers Subagent Widget Polish Design

## Goal

Make the Details sidebar Workers/Subagents surface predictable and visually
consistent with the inline To-do card.

## Confirmed causes

- `ChatWorkersWidgetState::sync_activities` expands the first activity whenever
  the expansion map is empty, overriding the row renderer's collapsed default.
- The subagent row stopped mounting `render_activity_status` when the avatar was
  judged to make the status redundant, so running subagents lost their spinner.
- The Details To-dos widget duplicates the inline To-do row at a smaller
  `32/10/8` geometry instead of the inline `36/12/9` row height, horizontal
  padding and gap.

## Design

New workflow and subagent identities enter the expansion map as `false`.
Explicit user toggles remain keyed by stable activity id and survive reorder and
streaming updates. Chat switches still clear the local disclosure state.

Subagent rows keep the seeded avatar and restore the existing lifecycle status
renderer in a fixed trailing slot. Running rows therefore renew the shared
paint-local spinner lease; completed, failed and cancelled rows retain the
existing semantic icons. No transport or persisted metadata changes.

The Details To-dos rows adopt the inline card's canonical geometry: 36 px row,
12 px horizontal padding, 9 px gap, 15 px non-shrinking status circle and 9 px
glyph. Colors, copy, ordering and current/done semantics remain unchanged.

## Verification

- Unit tests cover all-new-activities-collapsed, explicit-toggle persistence,
  context reset, and the shared To-do geometry contract.
- GPUI rendering has no inspection harness, so the mounted spinner and optical
  alignment require a headed dev-app smoke with running and settled subagents.
- Final gates include the focused UI tests, full UI suite, workspace check,
  formatting, diff check and the Impeccable detector.
