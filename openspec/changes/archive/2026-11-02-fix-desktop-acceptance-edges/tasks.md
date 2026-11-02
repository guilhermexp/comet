# Tasks

## 1. Mac sync

- [x] 1.1 Apply `cloud-round13-mac-sync.patch` on `542af56a` and commit it separately (product tree `e04c1373`)

## 2. Product fixes (TDD: each regression fails on the previous code)

- [x] 2.1 GH1: rediscover the installation and rebuild the plan after the exclusive lease, before the owner gate or any mutation (regression `an_update_queued_on_a_replaced_installation_updates_the_current_one`, with A kept and A removed)
- [x] 2.2 GH5: `sync_browser_details` after a Chat switch restores the destination (regression `restored_browser_destination_closes_details_once`)

## 3. QA-only fixes

- [x] 3.1 GH2: probes from the live viewport and placement with in-region checks; non-default resize before the takeover and exit checks
- [x] 3.2 GH3: current native visibility required per exit sample; fixture-only negative mode `ZERON_FIXTURE_NEGATIVE=hide-native-during-exit` (Linux run: the hidden sample fails and the run exits 1)
- [x] 3.3 GH4: skip only X11 `BadWindow` for vanished traversal candidates; disposable X11 smoke with a destroyed foreign candidate
- [x] 3.4 GH6: default-root stage of `worktree_destination` restricted to Unix; saved-root and override stages stay cross-platform

## 4. Documentation

- [x] 4.1 Deltas for GH1 and GH5 synced to canonical specs
- [x] 4.2 GH7/GH8/GH10/GH11/GH12 refutations and GH9 duplicate recorded in the delivery review (no code change)

## 5. Native acceptance (Mac orchestrator)

- [x] 5.1 Positive `desktop-corrections-fixture` run (all themes): `viewport_resized_before_takeover`, probe-based takeover and exit checks, exit 0 — Mac: 63 checks and 51 real captures passed, with stable own-window identity and presented frames
- [x] 5.2 Negative run with `ZERON_FIXTURE_NEGATIVE=hide-native-during-exit`: `exit_13a-takeover-exit-midway_50_*` fails and the process exits 1 — Mac: only the two designated hidden-page checks failed; all 17 captures succeeded
- [x] 5.3 Affected suites: `harness_updates`, `restored_browser_destination_closes_details_once` (with the existing Browser/Details tests), `worktree_destination` — Mac passed 60/6/1 on the exact tree `e0fa4400` (receipts at `.tmp/verify/desktop-corrections/mac/acceptance-edges/{updater,browser-details,worktree-destination}.log`)

### Native run on `e0fa4400` (Mac, round 13): 5.1/5.2 not accepted
- **Outcome:** the four native runs (glass, opaque, light, negative) exited 1, and none is visual proof.
- **Captures:** all 17 captures per theme failed with "fixture window is not available". The capture relied on NSApp's key or main window, and the fixture had neither.
- **Hit checks:** many failed in a pattern consistent with a stale, previous-state native frame (for example the takeover controls hit the earlier Pane bounds). The parent's diagnosis is an inference from that evidence, not a proven product defect, and product geometry was not changed.
- **Negative run:** it hid the intended page but also failed unrelated checks and captures, so it is not the isolated negative.
- **Round 14 (fixture mechanism only, `desktop-corrections-fixture.rs`):**
  - The capture targets the fixture's own NSWindow number, resolved from its GPUI window handle on the main thread (`own_window_number`). It is checked once at start (`own_window_identity`) and re-resolved before every capture, which must return the same number and a visible window.
  - Before every state-dependent native sample and every capture, a presentation barrier (`present_current`) runs: `refresh`, `draw`, `present_if_needed`, then a fixture-only frame request/rendered revision must match.
  - `screencapture` and the uniform-frame check run on a background executor; only the window number and path cross threads.
  - All checks, the 17 states, probes, the resize, per-sample visibility, the negative mode and the release check are kept.
- **Linux/Xvfb evidence (not Mac acceptance):**
  - all 17 own-window captures now present non-uniform frames from the same X11 window, each with its presented frame recorded;
  - the negative run fails only `exit_13a-takeover-exit-midway_50_widget_clear`, plus the known no-window-manager resize checks.
- **Corrected Mac acceptance:** the fixture-only follow-up passed on code tree `1187d789b87b7560cca905d4cb67643e01d7e426`: three positive themes (63 checks/51 captures), plus the isolated negative (exit1, only the two expected failures, 17 captures). Stable window numbers, increasing capture frames, actual native page hits and in-region probes were checked. The real images were visually inspected; earlier failures remain intact.
- **Receipts:** `.tmp/verify/desktop-corrections/mac/native-presenter-final/{results,acceptance-and-cleanup}.json`; images and contact sheet in `.tmp/evidence/desktop-corrections/mac-native-presenter-final/`. Shipping code remains identical to the Mac 60/6/1 passing source; those tests were not repeated.
- **Final pre-commit review:** actual status, snapshot and review modality are recorded in `.tmp/verify/desktop-corrections/final-review-receipt.json`; the native acceptance itself is not final review approval.
