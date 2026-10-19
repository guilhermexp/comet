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
failed integration attempts. `all-targets-source-final.log`, exit 0. Host mobile/UniFFI generation ran successfully; API equality was observed only
after an additional terminal LF outside the canonical script. That comparison
did not pass the literal generated-file drift gate (corrected in C1 below).
No iOS linking or Swift execution was implied.
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

Historical host UniFFI regeneration matched API content after trailing-blank
normalization plus an extra terminal LF. That LF addition was not in
`build-core.sh`; the comparison was insufficient for its exact drift gate. Two
literal comparisons failed on the final LF and must remain failures. Headers
and modulemap matched. Original provenance/logs are preserved; C1 below fixes
the generated artifact without changing API or gate.


## Review continuation R1–R5 (2026-10-19)

Delivered baseline `f3457141` was independently reviewed. R1–R3 reproduced with
scaled inputs only, under subprocess AS1GiB/CPU15s/wall25s limits: depth16 false
DAG accepted, repeated 32KiB span allocated 34,382,733 native bytes, unbounded
logs/text collections, repeated model views and aggregate/count budgets missing.
R5 reproduced Max Mode defaults after an empty catalog rejection. Before logs
are preserved separately from repairs, including all failed development checks.

Repairs `20b49e44`, `e40e727a`, `c698d6c3` bound native visits/deadlines,
pre-copy text/log bytes, pre-import model materialization and share image views;
separate grouped iOS Chat sources from destinations; accept Cursor metadata and
models atomically under the credential-scoped cache lock. The focused gate
passed 411 harness +19 Cursor +41 Visual +5 native allocation tests. Safe bounded
reproductions passed after the repair. One fresh read-only candidate review
found no concrete surviving bypass/regression. iOS behavioral tests are added,
not executed here: UIKit/Swift/Xcode are unavailable.

The previous complete run's ETXTBSY failure remains historical evidence; no
Codex fixture/adapter/test source was changed. New complete workspace and actual
isolated app results, exact logs, retained failures, resource limits, commits and
native gaps are recorded in `.tmp/reports/review-20261019.md`. Do not infer native
acceptance or archive from these Linux checks. Original U1–U11 evidence above is
preserved and unchanged.

Review source `c698d6c3`: the single final complete workspace run passed exit0,
5,847 outer-target passes plus two child re-executions, 52 inherited ignores.
The prior ETXTBSY test passes in this run; its earlier failed run remains true.

Production app build and private Xvfb/software-Vulkan smoke passed exit0. Four
scaled invalid publications returned the expected budgets; EngineReady remained
responsive. Valid dashboard checked six interactions; actual UI selected30d,
updated the chart and displayed Mon210/104 after enabling the table. New
screenshots, binary hash and exact latencies are in the review report. Format
check and strict OpenSpec validation passed. Graft refresh remains unavailable127.


## R2 retained text closure after independent review

The previous review did not completely close R2: axes labels/defaults and
implicit tab/link copies bypassed byte accounting. The follow-up also found
numeric root children and canvas text as equivalent input routes. Earlier
reports/logs are preserved, not promoted into proof for this repair.

Regression commit `21ecf740` fails four grouped behavioral tests on the delivered
implementation: axes individual/aggregate/default labels, tabs+links, and numeric
children+canvas. Hard-limited before/after runs (AS1GiB/CPU15s/wall25s) prove
all seven cases; native draw allocation for axes drops from1,592,885 to44,069
cumulative Rust bytes, with each retained label clipped to8,000. Near-limit
x/y/z defaults are charged and explicit missing/empty values retain semantics.

Repair `d87d5736` reuses append_bytes/retained_text, including a per-canvas draw
counter. Focused Visual gate passed41 unit +9 integration tests; production app
build and actual private Xvfb/software-Vulkan smoke passed0. Six invalid
publications return the expected text budget errors with EngineReady responsive;
valid axes/canvas/dashboard pass, including six checked dashboard interactions
and actual30d/table UI. One fresh read-only candidate review found no concrete
surviving bypass/regression. Audit, logs, limits, hashes and screenshots live in
`.tmp/reports/review-r2-closure.md` and its sibling evidence directory.

No full workspace rerun: explicitly forbidden by the owner for this closure;
CI follows acceptance via the orchestrator. Previous complete-run results stay
historical evidence, not validation of current source. No push/release/CI dispatch
or archival. Native macOS/iOS acceptance remains open.


## CI closeout C1/C2 — PR25 / 2026-10-19

Observed runs39189505714 and39189505816 both evaluate HEAD9e1a6b74. C1:
`ios-tests` failed only in “Generated Swift bindings are current”; Rust mobile
core tests passed, and “Run iOS unit tests” was skipped, not a native pass.
The complete log ends with a sole terminal-LF diff on `// swiftlint:enable all`.
Existing host library SHA256ba81e0e4… was reused to regenerate UniFFI and apply
exactly the script sed/cmp/copy stage. Canonical530850 bytes match the old530851
bytes minus one terminal LF; no other byte/API changes, headers/modulemap equal.
Prior bindgen-final/provenance.json explicitly added an extra LF, proving API
content only, not the canonical gate. Its evidence remains unchanged.

C2: Windows native-platform-check run39189505816 reports51 unpeel-core Unix API
errors. Baselinee750f810 and HEAD9e1a6b74 share Unpeel tree
a59bca0a6931665574af47e635d5af09a7a47630 with empty diff. This corroborates the
orchestrator baseline proof: preexisting incompatibility, outside integration
scope. No vendor port, workflow/gate change or Windows rerun performed.

Full downloaded logs, job metadata, SHA256 manifest, canonical output/provenance
and exact byte assertion: `.tmp/reports/ci-20261019.md`. No expensive local Linux
or workspace reruns, no new iOS pass claimed. Orchestrator publishes this local
correction and follows the iOS rerun; native/manual acceptance stays open.


## Review F1–F4 — 2026-10-19 (Claude Code handoff)

Codex worker interrupted before delivering fixes; its partial tests were kept and
revised (F1 sizes rescaled to the chosen budgets, F2 fixture lacked canvas
height/button labels and the shared-allocator lock). Its `red-f2.log` was not a
reproduction and is not used as one. Commits `a29021fc` (F1), `e1d015f6` (F2
parser), `474a6473` (F2/F3 mobile + iOS), `3e72f696` (F4).

RED on unfixed product code, AS1GiB/CPU15s/wall25s: F1 a ~700-byte asset
allocated 72,780,792 / 288,785,100 native bytes and was accepted (six unit
cases reached collection); F2 all 16 duplicate surface pairs and the
control-driven rerender were accepted, and mobile emitted a display with ids
`[18101011682566982396, 18101011682566982396]`; F3 mobile emitted a
237,992-point display (RED variant disables only the new cap); F4 a parked
runtime accepted a visual and success preceded persistence. GREEN: all refused
with < 8 MiB native allocation, short visible errors, persisted-on-ack.

Focused gate: `cargo test -p zeron-visual -p zeron-mobile -p zeron-doc` exit0
(49+12 visual, 80 mobile, 191+1 doc) and `cargo test -p zeron-engine -p zeron-mcp`
exit0 (659 engine unit and all integration targets incl. e2e). Production
`cargo build -p zeron` exit0; private headless engine smoke (bwrap, no network,
daemon AS2GiB) exit0: two F1 and two F2 invalid publications refused, two valid
publications stored, F4 after stop/parked refused with uploads unchanged.
Independent read-only review found no verified defect; two concerns were fixed
(per-visit vs summed model budget; size cap after the error line).

Not run: Swift/XCTest (no toolchain on Linux; `VisualRasterTests` targets the
existing `ios-tests` lane), full workspace, CI. Native macOS/iOS acceptance and
archive remain open. Evidence: `.tmp/reports/review-f1-f4-claude-20261019.md`.


## Review F1/F4 closure — 2026-10-19

Independent rereview of `d4afa44a` accepted F2/F3 and found two P2 gaps,
both reproduced here before the repair (AS1GiB/CPU15s/wall25s).

F1: gltf 1.4.1 `read_indices()` is `None` both for an absent accessor and for
an explicit one whose view is too short (`accessor::Iter::new` fails its slice).
A 4M-entry sparse POSITION plus 3 u32 indices on a 4-byte view was admitted at
48 MB and fell back to implicit indices: 198,219,964 native bytes before the
triangle limit. Commit `0a06faf3` refuses such an accessor in the pre-pass;
GREEN < 8 MiB, readable/absent controls and the 20-primitive shared-accessor
control still accepted.

F4: the dedicated visual arm skipped `last_stream_activity`. Paused-time
regression `tests/visual_quiesce.rs`: on `d4afa44a` the turn parked on the old
deadline after an accepted visual (Idle at t+W+60s); commit `9877fde1`
keeps it Working there, parks at the renewed deadline, and a refused (stale
ticket) publication does not prolong the next turn. GREEN 5/5 runs.

Gate `cargo test -p zeron-visual -p zeron-engine` exit0; `cargo build -p zeron`
exit0; private headless smoke exit0 (binary SHA256 dfcf37006c99f31bd45d9af498b910f58089f56bd994e8696ee0f35287fb6705): F1 three invalid
models (incl. truncated indices) and F2 two duplicates refused, three valid
publications stored, F4 stop/parked refused, uploads unchanged, daemon alive.
No workspace run, Swift, bindings or CI. Evidence:
`.tmp/reports/review-f1-f4-closure-20261019.md`.

