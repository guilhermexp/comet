# Verification

- `cargo test -p zeron-ui --lib`: 1338 passed, 0 failed.
- `table_bleed_takes_what_it_needs_and_never_more` covers: needs less than the
  budget, needs more (clamped), already fits (no stretch), zero budget, and
  NaN/negative inputs.
- `a_narrow_viewport_never_buys_a_table_any_bleed` covers the derivation at a
  wide viewport, exactly at the column, narrower than the column, absurdly
  narrow and unmeasured. The budget is zero in every case where a bleed could
  push content off-screen.
- `cargo fmt --all -- --check`: clean.
- `openspec validate tables-bleed-past-the-prose-column --strict`: valid.
- NOT reviewed on screen. This is a layout change on a virtualized list; the
  suite proves the arithmetic, not that 1100px reads well next to a 736px
  paragraph, and not that retained row heights settle correctly after a
  viewport change.

## Correction recorded
An earlier answer in this session claimed the inline diagram's advantage came
from reclaiming the row gutters. It does reclaim them, but that is only +32px;
what actually keeps a diagram from scrolling is that it is FITTED, and
`INLINE_MERMAID_SCALE` (1.25) scales the drawing against its own natural size,
not against the column. A table has no equivalent because its column floors
cannot shrink. That is why the bleed had to be built rather than copied.

## Tuning pass
`TABLE_MAX_WIDTH` shipped at 1100 (~1.5x the 736px column) and was reviewed on
screen: the table broke the left edge of the reading column hard enough to read
as a separate document. Lowered to 900 (~1.2x, +82px per side). The mechanism
was correct — measured on the screenshot the table rendered at ~1.4x the prose
width, which is what 1100 specifies — so only the constant moved.

This is the first part of this change actually seen rendered; the height and
virtualization behavior on a viewport change still has not been exercised.
