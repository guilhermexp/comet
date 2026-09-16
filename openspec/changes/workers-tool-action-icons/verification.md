# Verification

- `cargo test -p zeron-ui`: 1333 passed, 0 failed.
- `workers_rows_are_identified_by_their_action` covers all 15 mapped actions,
  asserts no two of them share a glyph, and asserts the unknown/absent fallback
  is neither a guess nor the launch glyph.
- A subagent spawn (`spawn_agent`) is asserted to keep `material("robot")`, so
  the two genera stay distinguishable.
- `cargo fmt --all -- --check`: clean.
- `openspec validate workers-tool-action-icons --strict --no-interactive`: valid.
- Native review PENDING: gpui render has no harness, so the suite cannot judge
  how the Solar glyphs read at 14px in the row. Archive blocked on that review.
