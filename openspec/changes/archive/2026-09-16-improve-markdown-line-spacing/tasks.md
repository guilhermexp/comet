## 1. Implementation

- [x] 1.1 Adjust shared prose leading and inset chip geometry; update owner contract.
- [x] 1.2 Run existing Markdown/transcript checks and inspect native rendering.
- [x] 1.3 Validate and archive OpenSpec, recording evidence.

## Verification

- `cargo test -p zeron-ui --lib markdown`: 115 passed.
- `cargo test -p zeron-ui --lib transcript::`: 138 passed.
- `scripts/dev-demo.sh --slow` with `ZERON_MOCK_ELEMENTS=1`: rebuilt app and inspected native streaming and settled file-reference rows. Chip backgrounds have vertical separation; font and links retained.
- Dense-paragraph fixture was prepared in `/tmp/comet-markdown-spacing.md`, but the QA window exited before opening it; that additional visual case was not completed.
- Existing native layout/click/selection regressions passed; no dedicated new render harness added.
- `cargo fmt --all`, `git diff --check`, strict OpenSpec validation.
