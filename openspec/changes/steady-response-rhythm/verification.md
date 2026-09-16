# Verification

- `cargo test -p zeron-ui`: 1334 passed, 0 failed.
- `block_gap_groups_a_heading_with_what_follows_it` pins the asymmetry, the
  heading-after-heading case, the `Rule` case, the untouched prose pairs and
  both `None` sides.
- `split_sibling_gaps_match_live_internal_spacing` was strengthened: it now
  asserts the transcript's row gap EQUALS `render::block_gap` for the same two
  blocks, instead of a literal. A literal on the right would have let the split
  and unsplit rules drift apart silently — which is exactly the jump-on-settle
  this change must not introduce.
- `a_heading_after_a_tool_group_still_opens_its_section` covers the `None`
  neighbour through the real projection.
- The chip height change is enforced by construction (it binds to the
  `line_height` parameter), so there is no unit that can fail; it is listed as
  visual acceptance in the spec rather than claimed as tested.
- `cargo fmt --all -- --check`: clean.
- `openspec validate steady-response-rhythm --strict --no-interactive`: valid.
- Native review PENDING for both halves. The measurement that motivated this
  came from a user screenshot, not from a harness, and nothing here proves how
  20/6 reads on screen.
