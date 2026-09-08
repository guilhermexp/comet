# Tasks

## 1. Projection

- [x] 1.1 Require an absent `worktree_branch` before marking `isGroup` in
      `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`, naming the
      canonical predicate it restores.

## 2. Verification

- [x] 2.1 Assert `is_group == false` on the worktree child project in
      `worktree_lifecycle_registers_and_removes_the_child_project`.
- [x] 2.2 `cargo test -p zeron-workers-unpeel` (all green) and
      `cargo test -p zeron-ui` (1153 passed, 1 failed:
      `details_sidebar::usage::tests::weekly_tone_neutral_when_no_usage_or_no_weekly_window`,
      a pre-existing regression from `804d83fc` — `usage.rs` names nothing this
      change touches).
- [x] 2.3 `cargo fmt --all --check`.
- [ ] 2.4 Visual check on `scripts/dev-demo.sh`: `New worktree…` on a project
      yields a selectable row with the branch glyph and no session.

## 3. Closeout

- [x] 3.1 Record the group predicate in `crates/workers-unpeel/AGENTS.md`.
- [ ] 3.2 Archive the change once 2.4 is confirmed.
