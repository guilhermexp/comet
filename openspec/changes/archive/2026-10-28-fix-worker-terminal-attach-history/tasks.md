# Tasks

## 1. Terminal acquisition

- [x] 1.1 Add bounded retained-journal reads and current-size mode snapshots; add downstream tests for retained floors, growing journals, missing/slow sockets and invalid IDs; update frontier DOX.
- [x] 1.2 Remove initial resize/snapshot dependency while preserving atomic presentation and generation fences; add replay/resize consumer tests and update UI DOX.

## 2. Navigation

- [x] 2.1 Route stopped/Shift wheel locally, preserve live TUI protocols and reader position; add consumer regressions and reclaimed-history notice.
- [x] 2.2 Supply an isolated native terminal fixture without provider bootstrap, with large synthetic history and appended output; record source-derived standalone evidence and state native execution is deferred to parent.

## 3. Integrated verification

- [x] 3.1 Parent runs focused Cargo checks, formatting and native fixture after integration; record exact revision and outcomes.
- [x] 3.2 Parent runs the required review-pair gate on the integrated snapshot before final commit.

## Workflow follow-up

- Validate this change now; archive only after parent acceptance. No push, merge, tag or release in this branch.

Implementation and integrated verification completed; evidence and limits follow.

## Integrated acceptance

- Production consumer test `terminal_history` passed: 8 MiB over 32 bounded pages, retention-floor changes, missing/delayed journal, passive stale-manifest read, absent/wedged socket and an independently blocked mode request. The initial fixture accidentally observed constructor telemetry migration, not the passive reader; it now isolates the seam before client construction. Its synthetic Host reply now uses the real camelCase wire contract.
- 46 UI tests passed, including empty-gap recovery with matching/stale Host snapshots, stopped/Shift navigation, replay/resize fences and stable scrolling. A final focused run after the last edit also passed the Host-width and stale-snapshot recovery case.
- Isolated production terminal rendered 8,407,627 journal bytes in 789 ms, reached the earliest retained row by wheel, kept that frame byte-identical after appended output, and rendered a narrow panel. Captures/receipt: `.tmp/evidence/fix-worker-terminal-attach-history/native-3/`; earlier captures are retained as well. No Shell/engine/providers or live Worker were started.
- Source base: `1513c527f44d0d257c691345550d9a34a232251a`, integrated on `fix/runtime-regressions-20261028`; logs: `.tmp/verify/runtime-regressions-20261028/checks-3/`. Final vendor tree was recalculated with an isolated index and matches `54cd15248b3c74aca11181abb12c9e277a4ac574`.
- Both reviewers completed in `.tmp/verify/runtime-regressions-20261028/review-pair/5/summary.md`; secrets clean. Its sole remaining finding is refuted in the voice change's tasks. Prior valid findings concerning idle repaint, missing journals and empty-gap screens were fixed.
- Later review round 6 found a hot read loop when a missing journal had a nonzero cursor. The reader now returns early only for actual growth; otherwise it honors the bounded requested wait. The expanded consumer test failed before the one-line fix and passed afterward, including a 40 ms minimum wait and subsequent independent mode-IPC coverage. Red/green logs: `.tmp/verify/runtime-regressions-20261028/checks-4-{red,green}/`. Round 6 did not release the gate because CodeRabbit was quota-limited; the final complete pair remains required.
- Review round 7 completed both reviewers with secrets clean and found recovery canceled by new journal bytes. The expanded `empty_truncated_gap_restores_only_a_matching_host_screen` regression failed with “journal progress must not cancel pending screen recovery”; after preserving a boolean recovery request until a snapshot matches the current consumed cursor, all 46 focused UI tests passed. The same case rejects a stale snapshot, restores a subsequent matching full screen and consumes following bytes without duplication. No journal bytes are skipped and mode IPC still does not block acquisition. Red/green Cargo output is preserved in the parent tool transcript.
- Review round 8's transient blank frame was reproduced separately: an empty truncated read erased “LAST KNOWN FRAME” before recovery. The consumer now retains its known frame when no replacement bytes/snapshot are available. The extended recovery case and all 46 focused UI tests passed afterward; matching Host geometry, stale-snapshot rejection, continued bytes and deferred recovery remain covered.
- Final production-component smoke was rerun after that fix: `.tmp/evidence/fix-worker-terminal-attach-history/native-4/` recovered 8,407,627 bytes in 1,172 ms; wheel reached the start, appended output preserved the exact captured frame, and the narrow layout rendered. `.tmp/evidence/fix-provider-limit-presentation/native-4.png` also rendered successfully. Red/green/build/native results: `.tmp/verify/runtime-regressions-20261028/checks-6/`.
- Round 9's stale VT-state finding was confirmed with a new consumer test: a matching primary-screen snapshot left the previous alternate screen active. Recovery now cancels stale painting/parser state and resets alternate/origin/scroll-region/style/charset state in place before applying the full Host frame, instead of recreating the emulator and losing retained primary history. The test also verifies scrolling at the bottom uses the full screen and the prior primary history remains reachable.
- The new case failed before the fix; all 47 focused UI tests passed after it. Both native production fixtures were rebuilt and rerun successfully (`.tmp/verify/runtime-regressions-20261028/checks-7/`); the terminal recovered 8,407,627 bytes in 1,434 ms in `.tmp/evidence/fix-worker-terminal-attach-history/native-5/`.
- Round 12's insert-mode concern was reproduced: after recovery, overwriting “HOST SCREEN” yielded “NEWHOST SCREEN” instead of “NEWT SCREEN”. The in-place reset now disables IRM (`CSI 4 l`). The extended consumer case and all 47 focused UI tests passed; native components were rebuilt and rerun in `.tmp/verify/runtime-regressions-20261028/checks-8/`, with final captures in `native-6/` (8,407,627 bytes in 730 ms).
- **Refutation — additional autowrap reset (round 12):** `.tmp/verify/runtime-regressions-20261028/review-pair/12/summary.md` conditionally asks to reset `?7` if Host ANSI does not restore it. The production converter `crates/workers-unpeel/src/lib.rs:1620-1661` always disables autowrap while painting and unconditionally emits `CSI ? 7 h` afterward (line 1652). The regression also starts from stale `?7l` and uses that real framing convention. An additional pre-frame enable is redundant; IRM, which the converter does not restore, was fixed and verified separately.
