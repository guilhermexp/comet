# Design

## Context

Fork base `eefb5e02e53c76ec6c8adf853adf05513bc89842`; common ancestor `54954e4d22bd4d78b2e2c7160a913b1af548dc79`; frozen upstream `75b72602a0348f5697c7affc9f84afe367b763fb`. The incoming interval changes 103 files and has 23 conflict paths. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:** semantic three-way resolution, real ancestry, full specified Linux gate and traceable adaptations.

**Non-Goals:** remote publication, main integration, app build, native install, real provider configuration access, widening timeouts or skipping failing tests.

## Decisions

1. **Engine and wire — ADAPT #876/#530.** Accept scratch cwd capability/home-index exclusions and per-device worktree settings; merge them into fork ProcessRunner, authorization and common Workers worktree lifecycle. New worktree settings RPCs go only in `crates/rpc/src/method.rs`. Preserve grants and fork fields in literals. Merge incoming tests with fork helpers instead of dropping them.
2. **Shell/navigation — ADAPT #867/#866/#725, TAKE #821/#774.** Accept responsive Session information, PR side panes, harness icons, bounce and file multiselect. Preserve live-tab right-pane semantics, Details metadata ownership, own panel/composer geometry, Workers navigation and right-side terminal. Persist new settings through `apply_shell_settings`. Worktree settings live in the existing General card/capture route, not a new Settings section.
3. **Browser — TAKE #573 with fork integration.** Retain account/profile-isolated persistent stores, restoration and storage fixtures; don't change native fractional clipping or focus lifecycle. No zui pin change exists in the incoming root manifest; keep current vendored provenance and punctuation rules untouched.
4. **Usage — ADAPT #873.** `account_usage.rs` stays deleted: port reset hover information to the existing Details Usage owner, not the removed upstream context/usage ring. Accept Accounts tooltip improvements with managed provider ordering and usage_lines preserved.
5. **Platform/client — TAKE #875/#843/#842/#812.** Keep iOS grouping, two MCP worker threads and entitlement/AutoFill updates while retaining fork identity, daemon/Workers dispatch and packaging behavior. Keep the refused `.github/workflows/macos.yml` deleted; preserve the fork's path gates in `ui-tests.yml` and include relevant browser storage coverage there.
6. **Manifests — SKIP b40ac5ee version bump.** Version remains 0.2.18; cargo regenerates the reconciled lockfile from resolved manifests, without broad dependency updates or replacing path zui dependencies.
7. **Verification/delivery.** Commit real merge before compile repairs. Audit additions per each incoming commit against upstream final, classifying adapted/missing lines rather than comparing whole divergent trees. Run the specified gate directly on Linux, record exits, compare failures in a disposable detached base worktree, fix causes without weakening assertions. Final history uses `outcome: prepared`, `delivery.integration: pending`; final report and gate logs are ignored local artifacts.

### Incoming commit classification

| Commit | Decision | Scope / reason |
|---|---|---|
| `edb23db4` | TAKE | iOS device-local Project grouping; no changes to Chat/session wire semantics |
| `1bc74d5d` | ADAPT | Scratch folders and home exclusion, preserving fork grants and workspace authorization |
| `50e33711` | TAKE | Two workers in each MCP stdio bridge; grant admission remains engine-owned |
| `9d943019` | ADAPT | AutoFill opt-out accepted in both bundle templates; fork microphone wording retained |
| `e54c3a86` | TAKE | Unified unsandboxed entitlements for dropped attachments; fork packaging/install policy unchanged |
| `b40ac5ee` | SKIP | Fork package version is 0.2.18; no upstream release bump |
| `c8c71786` | ADAPT | Responsive card without duplicating visible Details location metadata |
| `09beace0` | ADAPT | Reset hover information in Details Usage/Accounts, never restoring the removed usage ring |
| `053ce2ea` | ADAPT | Persistent isolated browser profiles; refused macos.yml remains deleted, storage checks port to fork ui-tests lanes |
| `66524017` | ADAPT | PR panes retain owning-device credentials and fork panel/navigation state |
| `2d40f8df` | TAKE | Overscroll applied to existing compact transcript/sidebar/Files |
| `2c2963aa` | TAKE | Multi-entry selection, drag and mutation in the single shared Files explorer |
| `01a399db` | ADAPT | Engine Chat location preferences feed the common lifecycle; standalone Workers defaults remain unchanged; bespoke Windows creation is refused under the existing fork platform policy |
| `75b72602` | ADAPT | Idle harness icons retain fork side-chat/subagent tab behavior |

The #530 preference applies to new engine-owned Chat worktrees, as in upstream.
It does not introduce a second preference service or change standalone Worker launch
destinations. Both callers continue sharing creation, ownership and preparation.

### Resolution evidence

- Real merge `b0c76e48e61e801ab815df0c7dd39a0c35bcb59c` has parents
  `e81fea005d7deed5ba6945ceb5fa01007e0f3b5c` and the frozen upstream.
- Upstream-final blame attributes 9,943 surviving addition lines to the 14
  incoming commits. Line-normalized multiset screening found 9,645 present
  and 298 candidates (69 formatting, 229 adaptations/refusals); all were
  inspected/classified. This is screening plus semantic
  review, not a claim that line presence proves behavior. Per-line/per-commit
  evidence is in the ticket's ignored `incoming-audit-*.json` artifacts.
- Per-device worktree preferences do not replace common repo hash layout or
  ownership journal. Preparation queries the journal after exact Git
  registration even when preferences change; no-record is external/no-op and
  corrupt state fails closed. Existing hooks/setup and ProcessRunner remain.
- The Windows-only two-step/long-file algorithm and its dependent new test
  are refused with that feature under the established platform policy, not
  hidden by ignored tests. Hook/create-failure tests exercise the common
  lifecycle; source-control tests create against a real local Git remote.
- Session information fit includes visible Details and the fork's 16px
  gutter, and animations use the Shell clock. Shared utility chips keep the
  fork's 118px slots, doc-keyed subagent avatar and separate spinner; only the
  scrolling region shrinks naturally. PR tabs/cache keys include target device.
- `cargo update --workspace --offline` regenerated the resolved lock with
  zero package-version changes; `same-file` was already locked and is now
  used by the engine. Vendored zui and both punctuation rules were untouched.
- Real Linux WebKit storage smoke passed all seven persistence/isolation/
  deletion/locking/permissions groups using an isolated helper under Xvfb.
  It neither launches the app nor accesses real provider configuration.
- Initial workspace check failed with three UI merge diagnostics; the same
  check passed on detached `eefb5e02`. Repair `074c53ef` restores the fork's
  live composer-width calculation and supplies active-state arguments to the
  new PR buttons. UI lib then passed 2,478 tests, with one upstream native
  WebKit test marked ignored until its explicit Xvfb execution.
- Real worktree smoke on `074c53ef` reproduced a false ancestor-root rejection:
  the inherited guard predicted the upstream basename destination instead of
  the common hashed directory. Repair `2965a58b` retains actual nesting
  protection. The original rejection/no-mutation assertions now use a truly
  nested fixture, and sibling/ancestor creation is checked against real Git
  registration. All five isolated smoke groups and that focused test passed:
  persistence/reload, failed-setup ownership, preparation after changing
  preferences, idempotence and deletion. No timeout/assertion relaxation.
- Full ordered gate on source revision `2965a58b` exited zero for all five
  commands: UI 2,478 passed, engine 657 passed, proto 102 passed and RPC
  24 unit + 15 integration passed. External inherited ignored scenarios are
  recorded, not hidden or newly skipped. The native WebKit context test was
  explicitly run under Xvfb and passed; focused engine integration passed all
  25 tests. Main-spec validation passed all 63 specs, with only existing
  long-requirement warnings. Final documentary/archive HEAD verification is
  recorded in the local report and `gate.log`.

## Risks / Trade-offs

- Common worktree lifecycle is a fork seam; upstream location preferences must not bypass ownership/setup hooks.
- Browser persistence is credential-sensitive; isolation is required and native AppKit/storage and iOS proof are not provided by Linux unit tests.
- Render fixtures may need explicit focus/invalidation; prior sync test fixes are not permission to weaken new assertions.
- Native visual QA is unavailable by repo policy until provider configuration isolation is proven. Record that limit; do not launch the app.
