- [x] 1. Add regression test and observe failure.
- [x] 2. Remove intermediate summaries without hiding calls/details.
- [x] 3. Verify UI tests, build and native rendering; update DOX and guide; validate/archive.

Evidence: regression failed with `no intermediate count header` in `/tmp/comet-counts-red.log`, passed in `/tmp/comet-counts-green.log`. All 1,201 UI tests passed (`/tmp/comet-counts-ui.log`); cargo build passed (`/tmp/comet-counts-build.log`). Native review of the same recorded Chat shown by the user: `codex-shot-2026-09-09_13-33-06.png` shows calls directly and only the TurnSteps total.
