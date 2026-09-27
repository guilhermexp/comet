## Context

The fork adds sticky row projection, measured per-user geometry, a clipped full-width card and a sticky overflow dialog. Upstream 433aa148 renders right-aligned inline bubbles and five-line expansion. Shared viewport width currently resides in sticky state and also drives Markdown table width. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:** Restore upstream presentation and interactions while removing obsolete sticky work from scrolling.

**Non-Goals:** Engine/data changes, replacing fork URL-chip or appshot features, altering rail caching, changing generic transcript/composer edge fades.

## Decisions

- Port upstream user-body/fold behavior at the existing renderer seam; keep fork rich-content projection. Replacing the whole upstream file would discard unrelated fork features.
- Delete sticky projection, overlay and geometry invalidation rather than hide the layer. Retain viewport measurement independently for table width.
- Preserve own-send runway but remove sticky-only handoff state if it has no other consumer.
- Remove sticky-only dialogs/tests and update DOX contracts. Retain generic edge-fade support and tests.

## Risks / Trade-offs

- Fold animation can disturb virtualized anchors → use upstream anchoring behavior and focused fold/runway tests plus native expansion QA.
- Viewport measurement may accidentally disappear with sticky state → retain the measurement used by table layout and inspect resize paths.
- Rich content differs from upstream → preserve URL chips, selection, badges and attachment callbacks at their current seams.

## Migration Plan

No data migration. Rollback by reverting this UI change; durable prompts remain unchanged.
