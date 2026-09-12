- [x] Test Gray versus colored accents in both appearances before implementation.
- [x] Paint rounded inline-code backgrounds without changing text geometry.
- [x] Verify the native appearance, run UI tests/build/format checks and archive.

Testable seam: flattened Markdown text runs choose the background from the accent selection while preserving text and link ranges. Native visual QA verifies rounding and wrapped text. Gates: `cargo test -p zeron-ui`, `cargo build`, `cargo fmt --all -- --check`.

Native QA: production `render_tree` with resolved Gray dark/light and Blue dark themes, at 740px and 460px window widths. Captures: `/tmp/comet-gray-inline-native.png`, `/tmp/comet-gray-inline-wrapped.png`. Temporary fixture source retained at `/tmp/comet-gray-inline-review.rs`. The new test failed before the fix (Dark/Gray had no background) and passed afterward.

Final gates: `cargo test -p zeron-ui` — 1,218 passed; `cargo build` and `git diff --check` passed; `cargo fmt --all` applied.

The main app was reopened with the new build after checking idle Chat execution state. The user's `Verificar versão mobile do comet` Chat shows the rounded gray backgrounds on the actual message; screenshot: `/tmp/comet-gray-inline-main.png`. `cargo fmt --all -- --check` and strict main-spec validation passed.
