# Verification ledger

The local review report is `.tmp/reports/upstream-20261018.md`; its evidence
folder retains immutable command/UTC/revision/diff-digest/exit logs, baseline
failures, conflict chunks, per-U file manifests and screenshots. It is a review
handoff, not publication authorization or native acceptance.

## Reproducible regressions and repairs

- Edge Visual conversion: 2 failing new regressions before repair, then 64 unit
  and 34 workerd tests passing. Invalid paths cannot become unsafe text fallback.
- Development updater: new test fails on baseline e750f810, passes in integrated
  workspace. Explicit macOS dev opt-out leaves fork installed-app/feed policy.
- Archived writer: removing its single central persistence assignment reproduces
  the regression. Restore assignment; the persisted-setting regression passes.
- First complete workspace run at merge 348f8b01: 22 failures, retained in full.
  Resolve plain-folder authority, fork path-scanner quadratic trimming, math
  selection/style fixture assumptions and lane focus/watch lifecycle. Five
  Workers failures were missing read-only Git metadata in the private sandbox;
  identical binaries pass when that mount is supplied.
- Second complete run at b1ceaa10: three sidebar-focus failures and one flaky
  replay test. Preserve immediate sidebar focus and defer only lane mounting.
  The existing Workers fixture failed 13/20 unchanged-binary repetitions; the
  filesystem probe found identical full mtimes for 562/1,000 write pairs. Give
  distinct synthetic events explicit mtimes; all payloads/assertions retained,
  runtime code unchanged. Then 20/20 repetitions pass.
- Complete focused UI: 2,554 passed, 0 failed, 1 inherited ignore. Complete
  Workers library: 324 passed, 0 failed, 0 ignored. `sidebar-seed-focused.log`.
- Third workspace run at clean source e6a7ef19: 5,836 passed, 1 failed,
  52 ignored, exit 101. The sole failure is unchanged Codex test
  `external_voice_keeps_canonical_events_without_any_local_helper`, Linux
  ETXTBSY while spawning its copied executable. Codex adapter, test and fixture
  are byte-identical to baseline. Then three complete Codex suites (39 passed,
  4 ignored each) plus 20 exact repetitions pass: 23/23 runs, exit 0, without
  product/test changes. The complete failed run is retained, not called green.
  Actual-application evidence is recorded separately in the review report.

All-target executable compilation passed again on e6a7ef19 after three retained
failed integration attempts. `all-targets-source-final.log`, exit 0. Host mobile/UniFFI generation passed, including normalized equality
against the reconciled Swift binding. No iOS linking or Swift execution implied.
Six incoming math fonts are byte-identical to the pinned upstream assets.

## Incoming-line reconciliation

40,308 added final-upstream lines were checked for whitespace-normalized
membership in the integrated files. The final source audit has 172 candidates,
all inspected/classified; zero unclassified. This is an omission-finding aid,
not a semantic-equivalence proof. The exact entries are in
`incoming-audit-decisions.json`. Reasons include fork version/lock graph,
local-only method registry, fork transcript/Details ownership, layout gutters,
neutral code chips, source-selection groups and destination-specific link styles.

## Native and external limits

See [native-handoff.md](native-handoff.md) for exact macOS/iOS targets, simulator
commands and manual fold/safe-area/drag checks. Keep OpenSpec open: native
acceptance remains pending. No push, release, deploy, CI dispatch, PR mutation or
Mac access. No live provider/account/billing proof. The native Python runner
suite is skipped on Linux and is not counted as a pass. `graft` is absent; exact
Git/source-span review substituted and the failed graph refresh is recorded.
Arbitrary nonterminating GPU shader execution is not claimed to have a proven
wall-clock bound.

## Actual application proof on Linux

Canonical `scripts/e2e-smoke.sh` ran inside a private HOME/network/PID sandbox,
built the production binary, started the real Worker and two engines, and passed
cross-device queued Run, transcript and working→idle synchronization (exit 0).
Then the actual production binary ran under Xvfb/software Vulkan, with a private
Local profile, mock harness and synthetic data. All sandbox processes exited at
the end; profiles, screenshots, actions/RPC logs and builds remain available.

The first UI attempt selected Development scope and required an X11 resize to
produce the first frame. Preserve it as environment evidence. The second used
an explicit synthetic WorkOS client identifier solely to select Local scope,
without login/network or real credentials. Map/resize/focus triggered initial
paint; no product edit was needed. Literal assertions in the QA driver were
corrected to match actual diagnostics, and `xdotool type` was split from later
commands. Captures 45/46 showed the latest Chat; verified math captures are
49/50. These automation mistakes are not product failures or acceptance proof.

Observed/asserted: native dashboard controls/table/Reset; canvas pointer, 3D
orbit and animated shader; same-process Chat navigation; TeX clipboard source;
Fira and STIX Two/135% at wide/narrow widths; lane pagination/scroll, rename
confirmed through WatchChats, 40-finished fold and Details ownership; hover
card dwell/scroll/menu/drag dismissal; section drag order persisted across
restart; Recent Files empty-query/reopen samples; Archived toggle/restart;
Delete-all Escape retained three synthetic IDs, confirmation removed exactly
those IDs and preserved the active-ID set. Real ListWorkspaceDirectory accepted
a registered plain folder and detached checkout with newline, rejected an
unregistered sibling.

Live PublishVisual rejects an endless Luau loop (~401 ms including automation),
filesystem API access (~101 ms) and allocation over 64 MB (~201 ms). EngineReady
still responds; storage contains exactly the original four valid scripts.
Arbitrary hostile GLSL time bounds were not tested. Other four math faces and
asset lifecycle have executed unit/integration evidence, not manual native
pixel review. Screenshot sampling does not establish a 5–8 ms palette benchmark.

Final host UniFFI regeneration matches committed Swift after trailing-blank and
terminal-LF normalization. Two literal drift checks failed only on missing final
LF in Linux output; all API content matched. Headers/modulemap match the first
regeneration. Provenance and both failed comparisons remain in the report.
