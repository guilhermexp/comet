## Context
Widths resolve at layout, not while elements are built, so the bleed cannot be
computed from the resolved column width. It has to be derived from values the
caller already knows: the measured viewport (`Transcript::viewport`) and the
table's own natural column widths (`TableColumns::naturals`).

## Decisions
- `RenderOptions` carries `block_width` and `table_bleed_budget`. The transcript
  fills both; `RenderOptions::settled` defaults the budget to zero, so every
  other surface keeps today's behavior without knowing the field exists.
- Width on the bleeding container stays AUTO. `w_full` would resolve 100%
  against the column and merely shift the box sideways — the same trap the
  inline-diagram block documents.
- The bleed is `min(needed, budget)`, not `budget`: a table that already fits
  must not stretch, or narrow tables would drift out of alignment with the prose
  above them for no reason.
- Bounded by `TABLE_MAX_WIDTH` rather than by the pane. On a wide display,
  letting a two-column table run the full pane width puts a 1700px table
  directly under a 736px paragraph, which reads worse than a scroller.

## Risks
- Table rows get taller/wider, so retained row heights and the sticky turn
  geometry must invalidate on a viewport change. `set_viewport` already calls
  `invalidate_layout`.
- A table between `TABLE_MAX_WIDTH` and the pane width still scrolls. Accepted:
  that is the bound's purpose, not a gap.
