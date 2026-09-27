## 1. Prep

- [x] 1.1 Audit upstream delta (4 read-only audits).
- [x] 1.2 Worktree `../comet-sync-v0.2.94` on `sync/upstream-v0.2.94` from `origin/main@fd19213b`.
- [x] 1.3 Update feed fix before the merge (D2).
- [x] 1.4 `cargo fmt --all` on the fork side.

## 2. Merge

- [x] 2.1 Merge `refs/upstream/zeron-main@433aa148` and resolve conflicts under D1–D7.
- [x] 2.2 `cargo check --workspace --all-targets` clean.
- [x] 2.3 `cargo test --workspace` green: 4,570 passed, 0 failed, 40 ignored; edge typecheck + 45/22 tests. (`zeron-preview` WebRTC tests are environment-flaky here, same on `origin/main`.)

## 3. Ports after the merge

- [x] 3.1 Sync stability chain #544/#550/#552 (D6).
- [x] 3.2 Accounts: #546, Pi from #542, engine parts of #449 (D5).
- [ ] 3.3 Discard guards from #81 in `Repos::discard_files`; untracked dirs keep ignored files.
- [x] 3.4 Side chats core (D4).
- [x] 3.5 Diff header open-in-Files (#311), canvas terminals (#474), last Settings section (#541).

## 4. Closeout

- [x] 4.1 DOX pass on touched `AGENTS.md`.
- [ ] 4.2 Visual smoke (human).
- [ ] 4.3 iOS `xcodebuild test` (CI).
