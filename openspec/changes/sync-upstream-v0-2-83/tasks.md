## 1. Merge

- [x] 1.1 Audit upstream delta per area (8 read-only audits: composer, harness, sidebar/browser, files/review, transcript/sessions, iOS, visual, infra).
- [x] 1.2 `cargo fmt --all` on the fork side; create worktree `../comet-sync-upstream` on `sync/upstream-v0.2.83` from `main@85d23c96`.
- [x] 1.3 Merge `refs/upstream/zeron-main@d721f301` with the grafted `v0.2.29` base (D1); remove the replace ref.
- [x] 1.4 Resolve 138 conflicted files (1,339 hunks) under the D2–D4 policy.

## 2. Integration

- [x] 2.1 Register upstream RPC methods in `crates/rpc/src/method.rs` (D5).
- [x] 2.2 Rename `comet-syntax` → `zeron-syntax`.
- [x] 2.3 Fix cross-file fallout until `cargo check --workspace --all-targets` is clean (fork-only struct fields in upstream tests/examples, duplicated auto-merged fields/arms/tests, duplicated `opening` lock, `Shell::new` Workers wiring via `shell::test_shell`).
- [x] 2.4 Drop scheduled `cursor-sdk-update.yml`; keep fork `release.yml`.
- [x] 2.5 `cargo test --workspace` green: 4,046 passed, 0 failed, 34 ignored (network/credential-only), parallel.
- [x] 2.6 `npm -C edge run typecheck` and `npm -C edge run test` (45 + 18 tests).

## 3. Before promotion (human)

- [ ] 3.1 Visual smoke with `scripts/dev-demo.sh`: composer (steer vs queue panel, reference chips), sidebar sections/pins, Files explorer/editor, diff comments/horizontal scroll, right-pane terminal + Project Actions, settings pages.
- [ ] 3.2 iOS `xcodebuild test` (queue + Steer, appshots, transcript layout).
- [x] 3.3 DOX pass on `crates/ui/AGENTS.md`, `crates/harness/AGENTS.md` (resolver now prefers newest binary), `crates/engine/AGENTS.md` (WriteWorkspaceFile now imported), root `AGENTS.md` (upstream is `zeronsh/zeron`).
