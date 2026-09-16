## Why
A Markdown table wider than the 736px prose column falls back to its horizontal
scroller, so the first columns slide out of view and the table can only be read
by dragging sideways. A diagram never has this problem because it is FITTED —
`aspect_ratio` plus `object_fit(Contain)` shrink it to the block. A table cannot
shrink: `min_table_width` is the sum of per-column floors, below which the text
stops being legible.

Reclaiming the row gutters the way a diagram does (`mx(-COLUMN_GUTTER)`) buys
exactly 32px, because the row is `px(16)` around a `max_w(736)` column. That is
not enough to matter.

## What Changes
- A table may bleed symmetrically past the prose column, up to a bounded total
  width, and only as far as its own natural width actually needs.
- The bleed is per BLOCK: paragraphs, lists, headings and code keep the 736px
  column, exactly as only a diagram widens today.
- The bleed is bounded by the measured viewport, so a narrow window gets none
  and nothing is pushed off-screen.
- The horizontal scroller stays as the fallback for tables wider than the bound.

## Capabilities
### Modified Capabilities
- `turn-step-tool-groups`: bounded table bleed past the prose column.

## Impact
`crates/ui/src/markdown/render.rs` and the transcript's `RenderOptions`. Surfaces
that are not the transcript (the Markdown file preview) pass a zero budget and
are unchanged.
