# Tasks

## W1
- [x] 1.1 Settings card renders persisted state; first enable chooses a folder before saving (`settings/worktrees.rs` unit tests).
- [x] 1.2 Worker checkout root honors the saved preference with env precedence (`worktree_ownership` unit test + `tests/worktree_destination.rs` with temp HOME/data dir/repos).
## W2
- [x] 2.1 Creation-ordered sections with id tie-break (`activity_menu` tests with bursts, appends, transitions).
- [x] 2.2 GR19 native acceptance fixture `examples/workers-menu-fixture.rs`: real NSStatusItem/NSPopover from the real projector, bursts that force rebuilds keep native rows/frames/tags, real hit-test + `performClick:` reach the receiver as the exact project/session, new Worker appends, a section change moves only that Worker, footer unchanged; exit 1 before AppKit terminate on failure. Linux builds it and it refuses to run there (exit 2); the macOS run is the parent's.
- [x] 2.3 GR19 round 9 (first Mac run: popover never shown): the fixture opens an empty GPUI activation host, waits frame by frame within the same 3 s budget for a native anchor (status item visible, button in an on-screen visible window with a non-empty screen frame, app active) and toggles once only when ready; it records the anchor timeline and every NSPopover notification (with close reason) in `result.json` and waits on `DidShow`/`DidClose` instead of fixed pauses. Production `NativeMenuBar` unchanged. macOS rerun is the parent's.
- [x] 2.4 GR19 round 10 (second Mac run: anchor ready and app active, yet no `isShown`, no window and no popover notification after the toggle): the popover now opens through the product path — a native `performClick:` on the status-item button, the `TogglePopover` the bridge actually emits, then the controller's `toggle` — and the readiness guard requires the app to be active (Mac's fixture-only fix, folded in). Read-only `appshots-fixture` tracing of the existing popover op records hop scheduled, hop entered on the main queue, and state before/after `showRelativeToRect:`/`performClose:` into `result.json`. Production behavior unchanged. macOS rerun is the parent's.
## C1
- [x] 3.1 Strip inside the composer chrome; geometry test with Todo, typed text, narrow/wide, several cards; content preserved.
## B1/B2
- [x] 4.1 Browser activation closes Details; previews do not; manual reopen respected (GPUI Shell test).
- [x] 4.2 Server rows route internally; stale/external-preference/remote-preview cases (GPUI Shell test).
## U1
- [x] 5.1 Reconcile external install while waiting; redacted bounded timeout/failure output (engine tests).
## Round 2 (independent Mac-side review)
- [x] 7.1 MR1 (U1): the external-change version probe is cancellable; cancel clears intent, pending marker and queued lease at once and kills only the probe (engine test with a stalled probe).
- [x] 7.2 MR2 (W1): one creation resolves the saved root once and builds its journal from that root (`tests/worktree_destination.rs`, preference switched between resolution and journal).
- [x] 7.3 MR8 (B1): the first Browser and a hidden→visible pane reopen onto a Browser close Details; a manual reopen while the same Browser stays visible is kept (GPUI Shell test).
- [x] 7.4 MR9 (V3): the widget sits beside native views when the window holds it; otherwise it keeps the corner and the Browser mask and file-preview frame leave its bottom band uncovered (GPUI Shell test; native fixture).
- [x] 7.5 MR10 (B2): an explicit navigation clears a pending server; without one the pending server still resolves (GPUI Shell test).

## Round 3 (pair gate)
- [x] 8.1 GR1 (U1): the external-change reconciliation probes the executable the registry resolves now; a waiting update is released when the new version lives at another path, whether the old path remains or vanished (engine test).
- [x] 8.2 GR4 (V3): placement budgets the capsule as rendered (154×62 px from the same style constants); exact boundary and one pixel short are covered (unit + GPUI).
- [x] 8.3 GR6 (W1): Workers resolve the app data dir by the app's rule, including `%USERPROFILE%\AppData\Local\Zeron` on Windows (pure resolver test).
- [x] 8.5 B2 (current upstream `2500e8b3`): links pasted in the user's own message (GitHub/YouTube chips) route through the Chat link router like Markdown links; default internal, explicit external preference honored (GPUI Shell test). Comparison evidence in the round-3 delivery.
- [x] 8.4 GR12 (W1): the MR2 switch-point seam exists only under `cfg(test)`; the race regression runs as a library unit test on the real creation path.

## Closeout
- [x] 6.1 AGENTS contracts updated; focused suites green; `openspec validate --strict`.
- [x] 6.2 Mac acceptance: UI 1025 focused tests, full Workers 543 and Core 740 passed; affected engine updater/SpawnChat, doc, harness, OMP RPC and MCP suites passed. Real native voice/widget/composer/WebKit presenters produced 45 captures across three themes with 21 checks; the real menu status-button/popover/bridge produced four visible WindowServer captures and passed all 18 checks, including five exact target clicks and explicit close. Default app build/help/version and real MCP/PTY smokes passed. Receipts: `.tmp/verify/desktop-corrections/mac/{final,qa-final,backend-final,round2,operational-final}/results.json` and `mac/w2-close-final/acceptance-and-cleanup.json`; native evidence under `.tmp/evidence/desktop-corrections/{mac-native-final,mac-native-workers-menu-close-final}/`.
- Acceptance limits: local/remote routing and busy OMP lifecycle use controlled consumer tests, not a live remote device or real installer. No additional full app, provider-backed call/Chat, real microphone/audio or Windows process was launched, per the approved isolation contract. The native menu fixture retains all checks; its screenshot runs off AppKit, and teardown recognizes the actual DidClose of its one presentation even when deactivation closed it first.

Final Mac package review: `.tmp/verify/desktop-corrections/final-review-receipt.json` records the actual review mode, reviewed tree and automatic/manual summary path.
