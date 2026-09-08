# Tasks

## 1. Detection

- [x] 1.1 Widen `git_head_branch` into `git_checkout`, returning the branch plus
      the main repository a linked worktree points at, in
      `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`.
- [x] 1.2 Fill `worktreeBranch` and `parentProjectID` in the snapshot projection
      only where the registry left them absent.

## 2. Verification

- [x] 2.1 Extend `git_head_branch_reads_checkout_worktree_and_detached_head` to
      assert the main repository, and that an ordinary checkout reports none.
- [x] 2.2 `cargo test -p unpeel-core --lib` (669 passed, 0 failed),
      `cargo test -p zeron-workers-unpeel` (all green, including the new
      `worktree_registered_as_a_plain_project_is_projected_as_a_worktree`) and
      `cargo test -p zeron-ui` (1159 passed, 1 failed:
      `details_sidebar::usage::tests::weekly_tone_neutral_when_no_usage_or_no_weekly_window`,
      the pre-existing regression from `804d83fc`).
- [x] 2.3 `cargo fmt --all --check`.
- [x] 2.4 Probe against the real `~/.unpeel/app-state.json`: 9 of 47 projects
      project as worktrees with their branch, 4 of them nested under the
      registered repository they are a checkout of.
- [ ] 2.5 Visual check on `scripts/dev-demo.sh`: `jk-wt-correios` and
      `jk-wt-assets` nest under `JK Distribuição` with their branch names.

## 3. Closeout

- [x] 3.1 Record disk detection in `crates/workers-unpeel/AGENTS.md`.
- [ ] 3.2 Archive once 2.5 is confirmed.
