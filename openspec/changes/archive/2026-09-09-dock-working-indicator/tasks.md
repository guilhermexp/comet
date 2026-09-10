## Tasks
- [x] Dock the existing indicator and update owner contracts.
- [x] Build, run existing UI tests and inspect the native composer spacing.

## Verification
- `cargo test -p zeron-ui --lib`: 1208 passed (`/tmp/comet-docked-indicator-tests.log`).
- `cargo build`: passed (`/tmp/comet-docked-indicator-build.log`).
- Formatting and `git diff --check` passed.
- Native isolated mock: `codex-shot-2026-09-09_15-52-18.png` shows one Conjuring indicator directly above and aligned with the input, without a duplicate below the command rows. The reserved 24px strip retains a small gap to the pill.
