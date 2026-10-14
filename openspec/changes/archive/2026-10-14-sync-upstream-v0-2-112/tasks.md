# Tasks

## 1. Preparation

- [x] 1.1 Fetch the fixed upstream without tags and verify base/common ancestor and all 14 incoming commits.
- [x] 1.2 Read fork contracts, prior sync artifacts and write decisions before resolution.
- [x] 1.3 Format the fork before the real merge and commit any resulting changes.

## 2. Integration

- [x] 2.1 Resolve engine/proto/RPC/client and tests with fork grants, ProcessRunner and common worktree lifecycle preserved.
- [x] 2.2 Resolve shell/navigation/panels while retaining Details/Workers/Usage and Enter/Steer.
- [x] 2.3 Resolve browser/settings/Usage, preserving profile isolation and refusing the deleted usage ring.
- [x] 2.4 Resolve manifests/platform workflows/metadata; regenerate Cargo.lock, retain version 0.2.18 and vendored zui.
- [x] 2.5 Commit the two-parent merge and put subsequent compile repairs in separate commits.
- [x] 2.6 Audit every surviving addition from all 14 incoming commits, recording adaptations/refusals.

## 3. Verification and delivery

- [x] 3.1 Run fmt check, all-targets workspace check, UI lib, engine lib and proto/RPC tests in order; compare failures to eefb5e02 in an isolated disposable worktree and fix causes without weakening tests.
- [x] 3.2 Exercise changed real paths through an isolated smoke command; document unavailable native visual/macOS/iOS proof.
- [x] 3.3 Reconcile DOX and validate/archive completed integration artifacts.
- [x] 3.4 Commit prepared history and short fork changelog; update sync recipe only for new durable decisions.
- [x] 3.5 Produce final report/gate log, prove merge ancestry/clean state and remove only task-created temporary worktrees; no push or app build/install.
