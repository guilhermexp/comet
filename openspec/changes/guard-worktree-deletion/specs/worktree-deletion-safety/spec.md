## ADDED Requirements

### Requirement: Deletion touches only a linked worktree

`delete_worktree` SHALL resolve its target to a linked worktree of the named
repository before removing anything, and SHALL reject the repository root and
any path outside that set. The resolved canonical path SHALL be the one used
for the branch read, the git removal and the direct-removal fallback, so no
caller-supplied string reaches the filesystem.

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

Test: `crates/engine/tests/m5_repos_diffs_terminals.rs` DeleteWorktree coverage

- **WHEN** deletion names a worktree the engine created
- **THEN** it is removed and its `zeron/…` branch is deleted
