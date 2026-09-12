# Change: A worktree deletion cannot reach a directory that is not one

## Why

`Repos::delete_worktree` accepts `worktreePath` from the caller and validates
nothing:

```rust
let removed = self.git(&["worktree", "remove", "--force", &worktree_path…], Some(repo_path)).await;
if removed.is_err() {
    // git refused (or the dir is half-gone) — delete the folder directly.
    let _ = std::fs::remove_dir_all(worktree_path);
}
```

The fallback fires exactly in the dangerous case. `git worktree remove` refuses
a path that is **not** a linked worktree — the repository's main checkout ("is
a main working tree"), or any unrelated folder ("is not a working tree") — and
the next line then deletes that folder recursively.

`DeleteWorktree` is `forwardable: true` (`crates/rpc/src/method.rs:232`), so
those parameters can arrive from another device over the relay. A wrong
`worktreePath` destroys the user's checkout.

The engine already owns the check: `Repos::workspace_checkout` resolves whether
a path is the repository root or one of its linked worktrees, and the three
chat-cwd paths go through it (`rpc.rs:613,638,674`). Only the destructive one
does not. Unpeel's own equivalent refuses the same case outright ("refusing to
remove a worktree Unpeel does not manage").

## What Changes

- Deletion resolves its target through the existing authorization helper and
  accepts only a linked worktree of the named repository — the repository root
  is rejected too, since removing it is never what deletion means.
- Everything downstream (the branch read, `git worktree remove`, the
  `remove_dir_all` fallback) uses that resolved canonical path, so no unchecked
  string reaches the filesystem.
- A worktree whose directory is already gone still prunes: with nothing on disk
  there is nothing to delete, so the walk skips straight to `worktree prune`.

## Capabilities

### New Capabilities

- `worktree-deletion-safety`: which paths a worktree deletion is allowed to
  touch.

### Modified Capabilities

None.

## Impact

- `crates/engine/src/repos.rs`: the authorization helper gains a
  root-rejecting sibling (`linked_worktree_checkout`), and `delete_worktree`
  resolves through it.
- No wire change: `DeleteWorktree` keeps its parameters and its reply. A
  caller that names a real worktree sees no difference; one that names
  anything else now gets an error instead of a deleted folder.
- Out of scope: `SwitchRef` takes `repo_path` from the caller without the same
  check. Its worst case is a `git checkout` in an unrelated repository, not
  data loss — worth its own pass, not a rider on this one.
