# Verification

- `cargo test -p zeron-ui`: 1331 passed, 0 failed.
- `cargo test -p zeron-ui --lib transcript::`: 134 passed, 0 failed.
- `cargo fmt --all`: applied.
- `openspec validate expand-streaming-command-payloads --strict --no-interactive`: valid.
- Native review with `scripts/dev-demo.sh` is PENDING — gpui render has no
  harness, so the green suite is not evidence the live turn looks right. Archive
  remains blocked on that review.

## File cards (second pass)
- `cargo test -p zeron-ui`: 1332 passed, 0 failed.
- New `file_cards_open_while_the_turn_is_live_and_close_on_settle` asserts the
  flag AND that the row version flips across the settle (same row id, so only
  the version can force the closing repaint).
- Still pending native review: the 200px live card is a layout change the suite
  cannot judge.
