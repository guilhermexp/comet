- [x] 1. Reproduce command summary regression with retained invocation.
- [x] 2. Apply compact command headers, joined expanded frame and reasoning rule.
- [x] 3. Validate UI tests/build and native render, update owner docs, validate/archive.

Evidence: failing command projection regression `/tmp/comet-command-red.log`, green `/tmp/comet-command-green.log`; UI suite 1,204 passed (`/tmp/comet-command-ui.log`); build passed (`/tmp/comet-command-build.log`). Native review of recorded Chat: `codex-shot-2026-09-09_14-00-23.png` shows compact summaries and inset reasoning; `codex-shot-2026-09-09_14-01-34.png` shows expanded command header and payload in one frame. No provider prompts sent.
