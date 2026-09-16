## Context
Three call sites stack Markdown blocks and must agree:
- `render::render_tree` — one flex column with a uniform gap.
- `transcript::top_gap_for` — the transcript splits a message into one row per
  top-level block, so the gap between siblings is a row gap, not a flex gap. Its
  own doc comment requires it to match the live row's internal spacing exactly,
  "so the live→split handoff cannot shift a pixel".
- `file_preview::view` — a virtualized list padding each block row.

## Decisions
- One `render::block_gap(previous, next)` consumed by all three. Splitting the
  rule across them is what would let the live and settled transcript drift, and
  that drift is visible as a jump at the moment a turn settles.
- The chip minimum binds to the `line_height` already passed into
  `inline_chips::render`, not to a literal 22. A chip inside an h2 then gets the
  h2's 24px box for free, and the constant cannot fall out of sync with
  `MD_LINE_HEIGHT` again.
- A heading's gap is asymmetric on purpose: `MD_SECTION_GAP` (20px) above,
  `MD_HEADING_LEAD` (6px) below. Heading-after-heading takes the 6px lead, so a
  subhead sits tight under its parent instead of being pushed away by the
  before-a-heading rule.
- `block_gap` takes `Option<&Block>` on both sides so the transcript can pass a
  non-Markdown neighbour (a tool group) as `None` and still give a following
  heading its section gap.

## Risks
- Answers get slightly taller. Bounded: only headings and thematic breaks
  change, and the 6px lead gives part of it back.
- The chip's fill now touches the line box exactly. Its 6px horizontal padding
  and 6px radius are unchanged, so it stays a pill; only the vertical overshoot
  that was breaking the grid is gone.
