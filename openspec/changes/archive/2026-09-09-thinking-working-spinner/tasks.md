## Tasks
- [x] Reuse the working spinner in active reasoning and remove eval/MCP execution prefixes.
- [x] Update owner contracts and verify build, projection tests and native tool headers.

## Verification
- RED: existing lifecycle test fails with `(Evaluating, eval)` instead of the requested name-only projection (`/tmp/comet-tool-names-red.log`).
- GREEN: `cargo test -p zeron-ui --lib`: 1208 passed (`/tmp/comet-spinner-names-tests.log`).
- `cargo build` and formatting check passed (`/tmp/comet-spinner-names-build.log`).
- Native isolated mock screenshot `codex-shot-2026-09-09_15-43-15.png`: eval and MCP headers omit prefixes; completed Thought has no icon. Screenshot `codex-shot-2026-09-09_15-44-01.png` shows the shared working dot spinner. The brief active reasoning interval was not captured; its existing active guard directly mounts the same function, scale and theme as the working indicator.
