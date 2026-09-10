- [x] 1. Reproduce fixed-label/body-retention regressions.
- [x] 2. Implement fixed labels and tool tone hierarchy.
- [x] 3. Run tests/build and native review; update DOX/guide; validate/archive.

Final behavior: Thinking/Thought bodies default open and respect manual collapse; reasoning arrows always visible; 24px spiral only mounted while active; completed Thought has no SVG. Tool action/detail tones differ.

Evidence: initial reasoning regressions failed (`/tmp/comet-thought-red.log`); spiral cadence failed then passed (`/tmp/comet-spiral-red.log`, `/tmp/comet-spiral-green2.log`); default-open regression failed then passed (`/tmp/comet-thought-open-red.log`, `/tmp/comet-thought-open-green.log`). Final UI suite: 1,203 passed (`/tmp/comet-thought-final-ui.log`); build passed (`/tmp/comet-thought-final-build.log`). Native recorded Chat capture `codex-shot-2026-09-09_13-49-50.png` shows open reasoning, Thought without SVG, visible arrows and distinct tool tones. The spiral geometry was also visible in the user's 13:44:12 capture before its completed-state removal.
