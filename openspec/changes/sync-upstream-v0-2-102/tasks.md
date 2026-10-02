## 1. Prep

- [x] 1.1 Audit upstream delta (56 commits, 46 conflicting files).
- [x] 1.2 Worktree `~/.zeron/worktrees/comet/sync-v0-2-102` on `sync/upstream-v0.2.102` from `origin/main@9a6b1bf5`.
- [x] 1.3 `cargo fmt --all` on the fork side.

## 2. Merge

- [x] 2.1 Merge `refs/upstream/zeron-main@64ad6f6e` and resolve conflicts under the fork contracts.
- [x] 2.2 `cargo check --workspace --all-targets` clean.
- [x] 2.3 `cargo test --workspace`: 5,022 passed, 1 failed, 41 ignored. The one failure is `executable::tests::version_probe_bounds_hangs_and_rejects_nonzero`, a parallel-run flake from upstream that passes alone (see `docs/upstream-sync.md`). Edge typecheck + 59/25 tests green.
- [x] 2.4 Line-by-line check that accepted upstream additions survived the resolution.

## 3. Closeout

- [x] 3.1 DOX pass on touched `AGENTS.md`; `docs/upstream-sync.md` updated.
- [ ] 3.2 Visual smoke (human): dictation, compact picker, file tree actions, Live Voice.
