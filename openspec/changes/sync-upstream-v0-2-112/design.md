# Design

## Context

Fork base `eefb5e02e53c76ec6c8adf853adf05513bc89842`; common ancestor `54954e4d22bd4d78b2e2c7160a913b1af548dc79`; frozen upstream `75b72602a0348f5697c7affc9f84afe367b763fb`. The incoming interval changes 103 files and has 23 conflict paths. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:** semantic three-way resolution, real ancestry, full specified Linux gate and traceable adaptations.

**Non-Goals:** remote publication, main integration, app build, native install, real provider configuration access, widening timeouts or skipping failing tests.

## Decisions

1. **Engine and wire — ADAPT #876/#530.** Accept scratch cwd capability/home-index exclusions and per-device worktree settings; merge them into fork ProcessRunner, authorization and common Workers worktree lifecycle. New worktree settings RPCs go only in `crates/rpc/src/method.rs`. Preserve grants and fork fields in literals. Merge incoming tests with fork helpers instead of dropping them.
2. **Shell/navigation — ADAPT #867/#866/#725, TAKE #821/#774.** Accept responsive Session information, PR side panes, harness icons, bounce and file multiselect. Preserve live-tab right-pane semantics, Details metadata ownership, own panel/composer geometry, Workers navigation and right-side terminal. Persist new settings through `apply_shell_settings` and include capture routing for Worktrees.
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

## Risks / Trade-offs

- Common worktree lifecycle is a fork seam; upstream location preferences must not bypass ownership/setup hooks.
- Browser persistence is credential-sensitive; isolation is required and native AppKit/storage and iOS proof are not provided by Linux unit tests.
- Render fixtures may need explicit focus/invalidation; prior sync test fixes are not permission to weaken new assertions.
- Native visual QA is unavailable by repo policy until provider configuration isolation is proven. Record that limit; do not launch the app.
