# Tasks

## 1. Targets

- [x] 1.1 `AppState` holds a second watch-target set and unions it into
      `reconcile_change_request_watches`, keeping the visibility gate and the
      unsupported-device filter.
- [x] 1.2 `WorkersModel` publishes one target per worktree project after each
      snapshot, addressed to the local device.

## 2. Lookup and row

- [x] 2.1 `ChangeRequestClientState` answers by `(cwd, branch)` for a
      device-local checkout.
- [x] 2.2 The Workers project row draws `pull_request_badge` beside the branch
      chip, under `sidebar_show_pull_request`.

## 3. Verification

- [x] 3.1 Unit: targets are exactly the worktree projects, and empty while the
      setting is off.
- [x] 3.2 Unit: the lookup matches on cwd AND branch, and refuses a snapshot
      whose branch moved on.
- [x] 3.3 `cargo test -p zeron-ui` (1161 passed, 1 failed: the pre-existing
      `weekly_tone_neutral_when_no_usage_or_no_weekly_window` from `804d83fc`),
      `cargo fmt --all --check` clean, `cargo build` clean (no warnings).
- [ ] 3.4 Visual check on `scripts/dev-demo.sh`.

## 4. Closeout

- [x] 4.1 Record the second target source in `crates/ui/AGENTS.md`.
- [ ] 4.2 Archive once 3.4 is confirmed.
