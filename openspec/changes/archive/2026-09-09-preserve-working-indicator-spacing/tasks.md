## Tasks
- [x] Preserve the previous footprint and update owner contracts.
- [x] Build, run UI tests and inspect native spacing.

## Verification
- `cargo test -p zeron-ui --lib`: 1208 passed (`/tmp/comet-indicator-reservation-tests.log`).
- `cargo build`: passed (`/tmp/comet-indicator-reservation-build.log`).
- Formatting and diff checks passed.
- Native isolated mock screenshot `codex-shot-2026-09-09_15-56-11.png`: content retains its clearance while the only visible working indicator sits directly above the composer. Blank reservation uses the previous label typography and 24px padding without mounting an animated spinner.
