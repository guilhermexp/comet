# Verification

- `cargo test -p zeron-ui`: full crate green.
- `transcript_chips_never_follow_the_accent` runs every `AccentColor::ALL` preset
  in both appearances, asserting the fill differs from BOTH `code_wash` and
  `accent`, and that it does not move when only the accent moves.
- The `Read` path chip was re-pointed at the same `chip_fill` helper. Its color
  is byte-identical to what it had (`text.opacity(0.06)`); routing it through the
  helper is what makes "the Workers chip matches the path chip" a fact of the
  code rather than two literals that happen to agree today.
- `cargo fmt --all -- --check`: clean.
- `openspec validate neutral-workers-tool-chips --strict --no-interactive`: valid.
- Native review PENDING.

## Correction
An earlier attempt in the same session changed Markdown LIST MARKERS instead,
from a wrong reading of a report whose screenshots had already been deleted from
the clipboard temp directory when they were read. That change and its OpenSpec
change were fully reverted once a readable screenshot arrived; `render.rs`
markers are back on `theme.accent.opacity(0.85)`.
