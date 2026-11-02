# Tasks

## 1. Mac sync

- [x] 1.1 Apply `cloud-round11-mac-sync.patch` on round-10 HEAD `06455a92` and commit it separately (product tree `a825de9f`)

## 2. Shipping-code fixes (TDD: each regression fails on the previous code)

- [x] 2.1 GF1/GF10: fingerprint before the waiting probe, verify after, pass it to `finish_already_current`; record a stable older probe (regressions: retarget during a current probe; older replacement probed once). Round 12 QA: the retarget regression no longer relies on elapsed time. It drives the watcher's reconciliation and waits, bounded, until the older binary is actually probed and recorded, failing at once if the update is released as Current; it still fails on the previous product code and passes now
- [x] 2.2 GF5: honor a present empty `ZERON_DATA_DIR` like the app; correct the resolver test; child-process proof with isolated HOME/cwd/data dir
- [x] 2.3 GF3: occupancy from the animated visible width (`native_right_inset`) for placement and crop; Shell regressions with motion on (close and fit eviction, beside-page and corner-band)
- [x] 2.4 GF4: remove the orb switch's custom key-down toggle; keyboard regressions for the switch and the widget controls (including unmounting return/end)
- [x] 2.5 GF7: render-time pending-server resolution loads without taking focus or replacing an edited address; regression for composer and address field
- [x] 2.6 GF9: advertise `MAX_KEYS_PER_CALL` in the `sessions` description and the key schema; disposable smoke of the real tool metadata
- [x] 2.7 GF13/GF20: `MenuBarRowOrder` for the popover (controller and native fixture); `project_activity_menu` back to recent-activity order for All recent; regressions for equal-time append and Recent vs menu bar

## 3. QA-only fixes

- [x] 3.1 GF2: Linux own-window capture (X11 `_NET_WM_PID`), explicit failure elsewhere; disposable X11 smoke of accept/reject/no-display
- [x] 3.2 GF6: native absolute temporary paths with `serde_json` in the saved-preference unit test
- [x] 3.3 GF15: Git isolation before `git init`/`commit` in `worktree_destination`, proven against a synthetic hostile signing/hook config. Round 12 QA: the isolation also clears all 16 repository-local variables (the `git_command.rs` list, mirrored in the test with no new API), and the hostile environment adds temporary `GIT_CONFIG`/`GIT_INDEX_FILE` sentinels whose bytes must stay unchanged; it fails with the round-11 isolation and passes now

## 4. Documentation

- [x] 4.1 GF12/GF14 and the other deltas in this change; sync to canonical specs, filling the three generated TBD Purposes (GF16/17) in the sync closeout
- [x] 4.2 GF18/GF19: repair the archived consolidate-chat-creation proposal and design wording
- [x] 4.3 GF8/GF11: record the refutations (no code change)

## 5. Native acceptance (Mac orchestrator)

- [x] 5.1 Updated `workers-menu-fixture` passed on the Mac: 20 checks, four visible popover-window PNGs, six exact native target clicks; the equal-time lexically earlier Worker appends without moving existing rows. Source tree `c6a6c0608e4faa92e3aa86d024b6f5c8d7df20b9`; real NSStatusItem/NSPopover/bridge, no Shell/engine/providers.
- [x] 5.2 Updated `desktop-corrections-fixture` passed in glass, opaque and light: 17 captures and 19 checks per theme (51 PNGs/57 checks), including both new exit frames and native page-hit protection at intermediate widths. Images inspected in `.tmp/evidence/desktop-corrections/mac-review-fixes/final-review-contact-sheet.png`.

Mac receipt: `.tmp/verify/desktop-corrections/mac/final-review-fixes/acceptance-and-cleanup.json` (16 steps passed; default app build/help/version, real MCP metadata, updater59, ownership13, destination1, UI263 and Core47). All 132 source paths were unchanged during the run; temporary test/runtime HOME and Git configuration were isolated. Shared Cargo/Rustup cache retained.

Final package review and actual review mode: `.tmp/verify/desktop-corrections/final-review-receipt.json`. No real provider/audio/OMP installer, live remote device, Windows process or additional full app was exercised; no main/push/release/install authorized.
