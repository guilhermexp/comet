## ADDED Requirements

### Requirement: A worktree never projects as a group

The `comet-local` project projection SHALL mark a project as a group only when
it is a folder, has a parent, and carries no worktree branch. A worktree
therefore SHALL reach the UI as a project that owns a checkout: selectable,
launchable, and a member of the project ledger.

#### Scenario: A registered worktree is not a group

Test: `worktree_lifecycle_registers_and_removes_the_child_project`

- **WHEN** a worktree is created through `create_worktree` and read back from
  `bootstrap`
- **THEN** the child project carries its `worktree_branch` and its parent
- **AND** the project is not marked as a group

#### Scenario: An organizational group stays a group

Test: `worktree_lifecycle_registers_and_removes_the_child_project`

- **WHEN** a folder project with a parent carries no worktree branch
- **THEN** it is still marked as a group
