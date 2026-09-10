## Tasks
- [x] Apply wrapping, natural height and spacing to tool payloads.
- [x] Run UI tests/build and verify native long command/output presentation.

## Verification
- `cargo test -p zeron-ui --lib`: 1208 passed (`/tmp/comet-wrap-ui.log`).
- `cargo build`: passed (`/tmp/comet-wrap-build.log`).
- Isolated native mock review: two consecutive expanded commands wrap and have separate borders, with natural payload heights. Screenshot: `/var/folders/vz/vrc2ttk13z9gk839jfrmxrbm0000gn/T/codex-shot-2026-09-09_15-34-08.png`.
- Fixture commands are display-only; no disk inspection commands were executed. The first long mock output is summarized by the existing 160-character doc limit; no change to that data policy.
