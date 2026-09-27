# Verification

Reference: upstream/main `433aa148d55e3316dab9d69afc402e0bb8f55583`, fetched on 2026-09-27. Changes are scoped to native user-message presentation; the upstream transcript file was inspected without replacing unrelated fork code.

## Regression baseline

Before implementation, the exact production `user_message_overflows` function and its two height constants were extracted with the new `user_message_fold_starts_after_five_rendered_lines` regression into a standalone Rust test. `rustc --test` followed by execution failed with exit 101 at `five lines fit in the collapsed message`: the fork's 100px card allowed only 84px of text, less than five 22px lines. This is an isolated pure-logic RED, not a full-crate test run.

## Focused validation

All Cargo commands used one `scripts/cargo-verify.py` invocation and its isolated target. Results on macOS:

- `cargo test -p zeron-ui --lib transcript`: 171 passed.
- `cargo test -p zeron-ui --lib edge_fade`: 2 passed.
- `cargo test -p zeron-ui --lib rail`: 36 passed.
- `cargo test -p zeron-ui --lib url_chips`: 9 passed.
- `cargo build -p zeron-ui --example user-message-fixture`: passed. The temporary native fixture is removed after QA.

## Native QA

Isolated synthetic Chat, no live messages sent and no restart of the user's running Zeron. Screenshots inspected through native computer-use tools on 2026-09-27:

- Opaque dark: right-aligned bubble, five visible lines, separate ellipsis, Show more reveals all seven lines, Show less restores the fold with its top at the same screen position.
- Opaque dark: scrolling removes the prompt completely, with no pinned copy. Short GitHub-chip prompt has no expander; the Markdown table renders correctly, including at the wider window size observed during QA.
- Frosted dark: same collapsed/expanded bubble and prompt fully leaves the viewport on scroll. No continuation blur overlays its text.

The first native launch exposed a reentrant `ListState::viewport_bounds` read inside row layout. It was fixed by independent viewport-width measurement with deferred remeasurement only on width changes. Both subsequent native launches completed successfully.

## Review

Independent review caught and resolved fold navigation discarding the own-send runway, and initial selection canceling the long-press timer. Regression tests cover both interactions. The independent viewport cache fix was reviewed with no material findings.

## Final checks

Production `cargo build` passed (2m 18s). `cargo fmt --all -- --check`, `git diff --check` and strict OpenSpec change validation passed.

Graft CLI is unavailable in this environment; saved graph nodes were consulted for orientation, then exact source spans were inspected. The graph could not be regenerated.

OpenSpec sync: all three deltas were checked against the resulting main specs; 58 main specs validated. The CLI archive parser treated existing `Test:` scenario annotations as unaccounted content when retiring the sticky capability, so the documented agent-driven sync was applied and checked before archival. The retired spec and Purpose remain recoverable from Git history.
