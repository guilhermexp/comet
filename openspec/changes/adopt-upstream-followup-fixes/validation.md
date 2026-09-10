# Validation — upstream followups

Fork base: `7d1615ab`. Upstream reference: `706debc06c02694223a04914ad8d84bf54dd2e73`.
Worktree: `comet-upstream-followups`, branch `feat/upstream-followups-20260910`.

## Applied scope

- R1: #304 (`2701c17c`) — fractional native browser frames and divider hit exclusion.
- R2: #302 (`d9fbf571`) — explicit one-shot composer/picker/terminal focus, preserving fork surfaces.
- R3: #303 (`45e68eef`) — restricted title entry points, isolated cwd, device preferences and settings UI. Preserves language, cheapest-current-family selection and shared deadlines.
- R4: #309 (`d6048a6a`) — persisted completion marker and notification consumption, adapted to existing steering/OMP/Live Voice. No shared-queue replacement.
- R5: #230 (`c80488a3`) — TS/TSX, Kotlin and Dockerfile syntax queries/injections. Existing caches retained.
- R6: #250 (`4c8099b4`, subset) — Codex reasoning paragraph/item boundaries per thread, including children. No tool restyling.
- R7: #257/#261/#262 (`5896a334`, `b004883c`, `3e96088d`) — layout-owned tail reservation, immediate wheel cancellation and follow intent. Fork sticky `has_landed` handoff retained. Selection-start interruption from #265 was already present and its regression test passes; user folding was not imported.

## Automated evidence

All Cargo calls were serialized with `CARGO_BUILD_JOBS=2`, `nice -n 10` and the existing shared target directory.

- Focused RED/GREEN: native divider hit testing; focus ownership; restricted title fake CLIs; completion vs steer/interrupt; syntax role coverage; reasoning boundaries; synchronous runway interruption.
- R7 first-paint geometry test passes after appending rows; reservation does not expose an intermediate scrollable gap.
- Complete workspace execution: 83 target summaries, 2715 passed, 1 failed, 18 ignored. The sole failure was the existing `run_controls_chat_id` recording fixture lacking the new opt-in `run_title` entry point. After updating that fixture, its target passed (1/1); production code did not change after this full execution. Combined final result: 2716 passing tests, 18 ignored.
- Earlier title integration failures were fixed before the complete execution above: retain explicit mock title generation despite mock not being a Settings-selectable harness; update inventory fixture to use the restricted entry point. Both title integration tests pass.
- `cargo fmt --all`, `git diff --check`, strict OpenSpec validation pass.
- `cargo build -p zeron --features browser-fixture --bins` passes.
- Opt-in native AppKit test passes: hide, descendant responder, repeated hide, Drop and unrelated responder. HTML host creation observed at 178.81 ms cold / 4.42 ms warm; these are fixture timings, not end-to-end interaction latency.
- Final production browser fixture passes native divider hit testing and fractional geometry.

Logs: `/tmp/comet-followups-workspace-tests-final.log`, `/tmp/comet-followups-native-build.log`, `/tmp/comet-followups-native-focus.log`, `/tmp/comet-followups-browser-final/result.txt`.

## Pending native interaction gate

BCU runtime `2026-04-20-window-motion-runtime` captured the isolated production preview fixture in background with the user's foreground application preserved. The screenshot shows Markdown, HTML and outside-checkout links plus the History pane. Both click attempts were rejected before dispatch by `stale_coordinate_guard`, including an immediate state recapture. No successful click, typing, preview open or latency measurement is claimed.

Evidence: `/tmp/comet-followups-preview-state.json`, `/tmp/comet-followups-preview-click.json`, `/tmp/comet-followups-preview-click2.json`, `/tmp/comet-followups-preview-after-click.json`.

QA processes launched for these checks were stopped or exited. No installed app was replaced, no push/deploy occurred. Keep this change unarchived and the branch isolated until native interactions can be verified and local promotion completed.
