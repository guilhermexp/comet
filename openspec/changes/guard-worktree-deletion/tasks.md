# Tasks

## 1. Authorization

- [x] 1.1 Add `linked_worktree_checkout` beside `workspace_checkout` in
      `crates/engine/src/repos.rs`, sharing one resolution that rejects the
      repository root.
- [x] 1.2 Resolve `delete_worktree`'s target through it and use the resolved
      path for the branch read, the git removal and the fallback.

## 2. Verification

- [x] 2.1 Test: deleting a sibling directory fails and the directory survives.
- [x] 2.2 Test: deleting the repository root fails and the checkout survives.
- [x] 2.3 Test: deleting a real worktree still works (existing coverage in
      `crates/engine/tests/m5_repos_diffs_terminals.rs` stays green).
- [x] 2.4 `cargo test -p zeron-engine` (all green, integration DeleteWorktree included) and `cargo fmt --all --check`.

## 3. Closeout

- [x] 3.1 Record the rule in `crates/engine/AGENTS.md`.
- [ ] 3.2 Archive once verified.
