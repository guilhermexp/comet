# OMP Subagent Details Parity Design

## Goal

Render OMP subagents in Comet with the same natural hierarchy used by
Orchestrator.dev, using only metadata emitted by OMP.

## Source Contract

OMP is the sole authority for subagent metadata. Comet consumes:

- `subagent_lifecycle`: id, status, agent, index and description;
- `subagent_progress`: assignment/task, tokens, tool count, duration, resolved
  model and progress state.

The existing OMP normalizer already maps those frames into
`WorkflowTaskUpdate`. The document projection already merges sparse lifecycle
updates with richer progress snapshots. No new metadata, inference or fallback
provider is needed.

## Presentation

The Subagents tab remains a compact operational list. Each row shows the OMP
description as its primary title, with `subagent_type` retained only as internal
classification. Status continues to come directly from the OMP lifecycle.

A row becomes expandable when OMP supplied usage or progress. Expanded content
reuses the existing workflow progress renderer:

- usage summary such as `1 agent · 1s` or available token/tool metrics;
- phase label from `phase_title`, including `OMP subagents`;
- child state dot;
- child description/assignment;
- resolved model.

Rows without progress remain one-line entries. Long text and models truncate
inside the existing sidebar width. Clicking the row continues to open the
subagent transcript; the disclosure control only expands/collapses details.

## State and Identity

Expansion is keyed by the stable OMP subagent task id and uses the existing
Workers widget expansion state. Live updates must not reset the user's choice.
The status icon remains visible in both collapsed and expanded states.

## Testing

Tests will prove that:

1. OMP subagent descriptions take precedence over the generic `task` type.
2. Usage and progress remain attached to the subagent row.
3. A subagent with OMP progress is expandable through stable task identity.
4. The progress renderer exposes the OMP phase, child label, state and model
   without synthesizing values.
5. Existing transcript-open behavior and worker/workflow tabs remain unchanged.
