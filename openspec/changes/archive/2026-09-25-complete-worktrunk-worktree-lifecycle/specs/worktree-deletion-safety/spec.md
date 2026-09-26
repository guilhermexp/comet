## MODIFIED Requirements

### Requirement: Deletion touches only a linked worktree

`delete_worktree` SHALL resolve its target against the linked worktrees of the named repository
before any filesystem mutation and SHALL use that resolved identity throughout. It SHALL refuse
the principal checkout, unrelated directories and any linked worktree lacking durable evidence
that the app created that exact checkout for that repository. Location under an app worktree root
or a `zeron/*` branch SHALL NOT substitute for that evidence. This rule SHALL apply equally to
local and forwarded RPC calls.

For a proven owned checkout, removal SHALL refuse a live Worker, a local `Working` Chat, untracked
or ignored files, tracked changes and HEAD not preserved by a ref. An approved `pre-remove` hook,
when present, SHALL run before the final cleanliness check so it can remove generated files;
identity and activity SHALL be checked again afterward. The operation SHALL NOT use `--force` or
recursive filesystem deletion. It SHALL delete the local branch only after proving it is integrated into the default branch and unchanged since that check; otherwise the branch SHALL remain, with an advisory warning for an inconclusive or failed cleanup.
For a registered worktree whose directory is already gone, it SHALL prune the stale Git
registration only after checking the stored repository/worktree identity, without deleting a
filesystem path.

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

Test: integration — `m5_repos_diffs_terminals` over a newly created, proven owned checkout.

- **WHEN** deletion names a clean worktree the engine created
- **THEN** the checkout is removed without force
- **AND** its local branch is deleted only if proven integrated

#### Scenario: An external worktree inside the root is refused

Test: integration — `DeleteWorktree` against a manually created worktree under the Comet root.

- **WHEN** a forwarded or local deletion names that worktree
- **THEN** the app refuses it for missing ownership evidence
- **AND** the checkout and branch remain

#### Scenario: A cleanup hook can make removal clean

Test: integration — owned checkout with ignored cache and approved `pre-remove` that removes it.

- **WHEN** removal runs the hook and the worktree is clean afterward
- **THEN** the checkout is removed and its branch follows the integration rule

#### Scenario: Local changes still block removal

Test: integration — owned checkout with untracked data left after any hook.

- **WHEN** the final cleanliness check finds the data
- **THEN** removal fails and the data remains

#### Scenario: A vanished worktree is pruned safely

Test: `detached_and_vanished_worktrees_stay_deletable`, extended for stored identity.

- **WHEN** a proven registered worktree's directory no longer exists
- **THEN** its stale Git registration is pruned
- **AND** no directory is removed
