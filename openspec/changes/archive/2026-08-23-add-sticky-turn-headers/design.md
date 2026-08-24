## Context

The GPUI transcript virtualizes a flat list of block rows. A user message is one `RowKind::User`, while the assistant work that follows can span many independent rows. `OwnTurnAnchor` reserves viewport runway and glides the latest local send to the top, but it is intentionally released by user input and retired when the response fills the reservation. Orchestrator.dev instead places the user bubble in a sticky element inside each message-group wrapper.

## Goals / Non-Goals

**Goals:**

- Keep the user card for the turn crossing the reading line at the transcript top.
- Let the next turn boundary push and replace the previous header.
- Reuse the complete user renderer, including attachments, mentions, badges, pending opacity, overflow dialog, and theme.
- Preserve row heights, minimal splices, render caches, streaming remeasurement, runway arrival, bottom pinning, file-card scroll containment, and chat switching.

**Non-Goals:**

- Regrouping the persisted transcript or changing message schemas.
- Replacing the own-turn runway or composer behavior.
- Adding sticky headers to read-only subagent transcript tabs.
- Introducing a separate visual design for sticky user messages.

## Decisions

### Paint-only overlay over the flat list

The transcript paints a cloned `RowKind::User` as an absolute sibling after the source card crosses the sticky inset. It calls the existing `render_row_body` with namespaced element ids. Because the overlay is absolute, it contributes no list height and does not invalidate virtualization or scroll anchors.

Wrapping every assistant block into a variable-height turn row was rejected because it would replace the established block-granular virtualizer and destabilize caches, folds, and streaming splices.

### Measured per-chat turn geometry

The row renderer records each visible user card's top, height, and scrollbar offset. When GPUI's bottom-aligned glued representation returns no `bounds_for_item`, the current top is projected from the measured geometry and scroll delta. Turn selection uses these measured boundaries before falling back to the logical top row. This avoids both the latest-global-header bug and a duplicate clone while the original card is still visible.

Geometry is render-local, pruned with rows, and cleared on chat attachment.
Reflows invalidate only user geometries after the changed row when that boundary
is known; global media/viewport reflows mark existing measurements invalid and
suppress an unbounded overlay for one frame while visible originals remeasure.
Viewport width/height changes invalidate geometry with a 0.5px dead band.

The outer `Transcript::render` samples the scrollbar offset before GPUI enters
the list prepaint borrow. Row processors receive only that scalar snapshot and
must not read `ListState` reentrantly.

### Boundary push and runway handoff

The overlay rests at the same inset as `OwnTurnAnchor`. Its top becomes `min(sticky_top, next_turn_top - header_height)`, so the next group pushes it away. The initial own-turn glide owns the visual until it lands; after a user releases and later resticks the runway, the overlay remains until the source card has re-landed.

## Risks / Trade-offs

- Measuring visible user cards schedules at most one correcting render when geometry changes; a 0.5px dead band prevents steady-state loops.
- A user card first encountered only as an unmeasured off-screen row uses the existing 100px card cap as a one-frame push-height fallback, then replaces it with measured height.
- The sticky copy paints the same content with distinct element ids; source row heights and render caches remain authoritative.
