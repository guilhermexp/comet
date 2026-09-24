## MODIFIED Requirements

### Requirement: Deletion touches only a linked worktree

`delete_worktree` SHALL resolve its target to a linked worktree of the named
repository before removing anything, and SHALL reject the repository root and
any path outside that set. The resolved canonical path SHALL be the one used
for every later step, so no caller-supplied string reaches the filesystem.

Once resolved, deletion SHALL follow the app's single removal policy: the
checkout SHALL be managed by the app (under the canonical or a legacy worktree
root), SHALL NOT be in use by a Worker or a `Working` Chat, SHALL have no
uncommitted, untracked or ignored files, and its HEAD SHALL be retained by a
branch or tag. Removal SHALL NOT force, SHALL NOT fall back to deleting the
directory recursively, and SHALL NOT delete the checkout's branch. A worktree
whose directory is already gone SHALL still have its registration pruned.

#### Scenario: An unrelated directory is refused

Test: `delete_worktree_refuses_paths_that_are_not_linked_worktrees`

- **WHEN** deletion names a directory that is not a worktree of the repository
- **THEN** it fails
- **AND** the directory is still on disk

#### Scenario: The main checkout is refused

Test: `delete_worktree_refuses_paths_that_are_not_linked_worktrees`

- **WHEN** deletion names the repository root
- **THEN** it fails
- **AND** the checkout is still on disk

#### Scenario: A real worktree is still removed

Test: `crates/engine/tests/m5_repos_diffs_terminals.rs` DeleteWorktree coverage, updated

- **WHEN** deletion names a clean worktree the app created, whose HEAD is on its branch
- **THEN** the checkout is removed
- **AND** its branch, `zeron/…` included, still exists

#### Scenario: Local changes block deletion

Test: integration — DeleteWorktree over a worktree with an untracked file.

- **WHEN** deletion names a managed worktree with uncommitted, untracked or ignored files
- **THEN** it fails and says the changes must be preserved first
- **AND** the files are still on disk

#### Scenario: An external worktree is not deleted by the engine

Test: integration — DeleteWorktree over a linked worktree outside the app's roots.

- **WHEN** deletion names a linked worktree that lives outside the app's worktree roots
- **THEN** it fails
- **AND** the checkout is still on disk

#### Scenario: A vanished worktree is pruned

Test: `detached_and_vanished_worktrees_stay_deletable`

- **WHEN** deletion names a registered worktree whose directory no longer exists
- **THEN** its registration is pruned
- **AND** nothing else is removed
