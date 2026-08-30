## 1. Native reentrancy diagnosis

Current-tree status: repeated headed interaction across Chat, subagent panes,
Workers, and native session menus did not reproduce the historical GPUI borrow
error. No speculative callback mutation was made; these tasks remain open until
a failing callback can be observed.

- [ ] 1.1 Re-read concurrent UI diffs, identify candidate nested window/entity updates, and add scoped callback identity to the native diagnostic path.
- [ ] 1.2 Reproduce `RefCell already borrowed` through a real headed interaction and record the exact callback without changing behavior.
- [ ] 1.3 Add the smallest failing state-transition regression available for the confirmed callback.
- [ ] 1.4 Remove the confirmed nested mutation or defer it to the correct GPUI boundary, then prove the focused regression and repeated headed smoke are clean.

## 2. Transcript writer integrity

- [x] 2.1 Add a schema test with sentinel content that requires a field-path/part-kind diagnostic and proves content is absent from diagnostics.
- [x] 2.2 Implement privacy-safe structural error metadata while retaining existing salvage behavior.
- [x] 2.3 Reproduce the observed missing-`id` shape, inspect persisted snapshots, and add a failing regression for the contentless incremental-import shell.
- [x] 2.4 Classify only identity-only incomplete shells as transient debug events and prove actionable salvage regressions remain green.

## 3. Closeout

- [x] 3.1 Update the nearest affected DOX verification matrices after the concrete files are known.
- [x] 3.2 Run focused UI/doc tests, formatting, workspace tests, app build, and the headed native smoke.

## 4. Local IPC handshake diagnostics

- [x] 4.1 Add a failing unit regression that distinguishes Tungstenite's incomplete-handshake variant from other protocol failures.
- [x] 4.2 Downgrade only the incomplete peer disconnect to debug and retain warning-level diagnostics for all other handshake failures.
- [x] 4.3 Update RPC DOX and rerun the RPC, workspace, formatting, build, and strict OpenSpec gates.

## 5. Change-request provider diagnostics

- [x] 5.1 Add a failing unit regression that distinguishes an unsupported checkout from actionable GitHub failures.
- [x] 5.2 Downgrade only `UnsupportedRepository` to debug while preserving backoff, last-success retention, and warnings for operational failures.
- [x] 5.3 Update engine DOX and rerun the focused, workspace, formatting, build, and strict OpenSpec gates.

## 6. Peer credential revocation

- [x] 6.1 Reproduce the scheduling-sensitive sign-out failure in the full workspace suite and identify the late `watch` subscription.
- [x] 6.2 Assert that `LinkCache` subscribes before construction returns.
- [x] 6.3 Move subscription ahead of supervisor spawn so sign-out closes cached authenticated links under runtime contention.
- [x] 6.4 Rerun focused RPC, workspace, formatting, build, and strict OpenSpec gates.
