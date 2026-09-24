## ADDED Requirements

### Requirement: A linked worktree is recognised from its checkout

The `comet-local` project projection SHALL treat a project whose folder is a
linked git worktree as a worktree even when its registry record carries no
`worktree_branch` and no parent. It SHALL report the checked-out branch as the
project's worktree branch, and SHALL nest the project under the registered
project whose path is the worktree's main repository. Values already present in
the registry SHALL win: detection only fills what is absent.

#### Scenario: A worktree registered as a plain project names its branch

Test: `git_head_branch_reads_checkout_worktree_and_detached_head`

- **WHEN** a project folder holds a `.git` file pointing at
  `<main>/.git/worktrees/<name>`
- **THEN** the projection reports the branch that gitdir has checked out
- **AND** reports `<main>` as the repository the checkout belongs to

#### Scenario: An ordinary checkout belongs to no other project

Test: `git_head_branch_reads_checkout_worktree_and_detached_head`

- **WHEN** a project folder holds a `.git` directory
- **THEN** the projection reports its branch and no main repository, so the
  project keeps its own position in the tree
