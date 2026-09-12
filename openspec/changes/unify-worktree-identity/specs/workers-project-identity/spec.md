## ADDED Requirements

### Requirement: A worktree launches its session in its own checkout

The `comet-local` creation catalog SHALL project a worktree as a project that
owns a checkout — not as a folder — and SHALL carry the same worktree path and
worktree branch the snapshot gave the UI. A launch aimed at a worktree
therefore SHALL NOT be rejected as a folder, and SHALL NOT be rejected as
belonging to another project when the payload echoes the snapshot the UI read.

#### Scenario: A created worktree accepts a launch

Test: `launching_into_a_created_worktree_is_not_rejected_as_a_folder`

- **WHEN** a session is launched into a worktree that `create_worktree` made
- **THEN** the request is not refused as "project is a folder"
- **AND** it reaches the launch arguments themselves

#### Scenario: An adopted worktree accepts a launch

Test: none — no test in the tree launches into a worktree whose identity came
from disk. Task 6.2 adds that probe to
`worktree_registered_as_a_plain_project_is_projected_as_a_worktree`, which today
asserts the projection only.

- **WHEN** a session is launched into a worktree whose identity came from disk,
  with the path and branch the sidebar read from the snapshot
- **THEN** the request is not refused as "worktree does not belong to project"

#### Scenario: A failed launch keeps the checkout it created

Test: `a_failed_launch_keeps_the_worktree_it_just_created`

- **WHEN** creating a worktree succeeds and the launch that follows fails
- **THEN** the error is reported
- **AND** the checkout stays on disk, as it already does for a failed setup

### Requirement: Removing a worktree deletes only a checkout the app created

Removal SHALL accept any worktree, however its identity was established, and
SHALL delete the checkout from disk only when the registry records the app as
its creator AND the path lies under the managed worktree root. Otherwise the
project is de-registered with its sessions and the folder is left alone. The
ownership predicate SHALL be one function, shared by the caller that asks and
the routine that refuses.

#### Scenario: An adopted worktree is de-registered, not deleted

Test: none — no test removes a worktree the app did not create. Task 6.2 adds
that half to `worktree_registered_as_a_plain_project_is_projected_as_a_worktree`;
the neighbouring `a_group_inside_a_worktree_stays_a_group` only proves that
removing a GROUP leaves its parent's checkout alone.

- **WHEN** a worktree the app did not create is removed
- **THEN** the project and its sessions leave the registry
- **AND** the checkout still exists on disk

#### Scenario: A worktree the app created is still deleted

Test: `worktree_lifecycle_registers_and_removes_the_child_project`

- **WHEN** a worktree created by `create_worktree` is removed
- **THEN** its checkout is deleted, as before

### Requirement: The pull-request badge follows the checkout

The branch a Workers project offers to change-request lookup SHALL be gated by
the registry's worktree branch — only a worktree has a pull request — and
valued by the branch on disk, so switching branches inside the checkout moves
the badge. Both halves of the pair, the subscription target and the read, SHALL
ask the same accessor.

#### Scenario: A switched worktree names the branch on disk

Test: `workers_change_request_targets_cover_worktrees_only`

- **WHEN** a worktree's registered branch and its checked-out branch disagree
- **THEN** the change-request target names the checked-out branch
- **AND** a project with no worktree branch, or a blank one, offers no target

## MODIFIED Requirements

### Requirement: A worktree never projects as a group

The projection SHALL decide, once per project, whether it is an organizational
group — a folder with a parent and no worktree branch in the registry — and
SHALL NOT apply disk detection to a record that is a group. Values already
present in the registry SHALL still win: detection only fills what is absent. A
worktree therefore SHALL reach the UI as a project that owns a checkout:
selectable, launchable, and a member of the project ledger. The group verdict,
not the presence of a parent, SHALL decide which removal a caller dispatches,
which verb the project menu offers, and which confirmation the sidebar draws.

#### Scenario: A registered worktree is not a group

Test: `worktree_lifecycle_registers_and_removes_the_child_project`

- **WHEN** a worktree is created through `create_worktree` and read back from
  `bootstrap`
- **THEN** the child project carries its `worktree_branch` and its parent
- **AND** the project is not marked as a group

#### Scenario: An organizational group stays a group

Test: `removing_an_empty_group_preserves_the_parent_and_unknown_state`

- **WHEN** a folder project with a parent carries no worktree branch
- **THEN** it is still marked as a group

#### Scenario: A group whose path is a worktree stays a group

Test: `a_group_inside_a_worktree_stays_a_group`

- **WHEN** a group is created on a worktree row, so its record inherits the
  worktree's path
- **THEN** the projection marks it as a group
- **AND** it names no worktree branch, so no removal route can reach the
  parent's checkout

#### Scenario: A child that is not a group removes as a project

Test: `adopted_worktree_without_a_branch_removes_as_a_project` in
`crates/ui/src/workers/project_menu.rs`, and
`a_child_that_is_not_a_group_removes_as_a_project` for the route the item
dispatches.

- **WHEN** the menu is built for a child project that carries no worktree
  branch and is not a group
- **THEN** the removal item removes a project, not a group

#### Scenario: An adopted worktree can be renamed

Test: `renaming_an_adopted_worktree_writes_the_project_record`

- **WHEN** an adopted worktree is renamed from the sidebar
- **THEN** the project record takes the new name
- **AND** a plain group renamed the same way still routes to the group rename

### Requirement: A linked worktree is recognised from its checkout

The `comet-local` project projection SHALL treat a project whose folder is a
linked git worktree as a worktree even when its registry record carries no
`worktree_branch` and no parent. It SHALL report the checked-out branch as the
project's worktree branch, and SHALL nest the project under the registered
project whose path is the worktree's main repository. Values already present in
the registry SHALL win: detection only fills what is absent.

Detection SHALL NOT promote a detached HEAD to a worktree branch. The
checkout's reported git branch SHALL keep saying what HEAD says — the short
sha — and the project SHALL keep its place in the tree rather than falling
back to a group.

#### Scenario: A worktree registered as a plain project names its branch

Test: `git_head_branch_reads_checkout_worktree_and_detached_head`, and
`worktree_registered_as_a_plain_project_is_projected_as_a_worktree` for the
projection that reads it.

- **WHEN** a project folder holds a `.git` file pointing at
  `<main>/.git/worktrees/<name>`
- **THEN** the projection reports the branch that gitdir has checked out
- **AND** reports `<main>` as the repository the checkout belongs to

#### Scenario: An ordinary checkout belongs to no other project

Test: `git_head_branch_reads_checkout_worktree_and_detached_head`

- **WHEN** a project folder holds a `.git` directory
- **THEN** the projection reports its branch and no main repository, so the
  project keeps its own position in the tree

#### Scenario: Detachment is reported by the checkout reader

Test: `git_head_branch_reads_checkout_worktree_and_detached_head`

- **WHEN** a checkout's HEAD names no branch
- **THEN** the reader reports the short sha and marks the checkout detached
- **AND** a checkout on a branch is not marked detached

#### Scenario: A detached worktree keeps its row

Test: `a_detached_worktree_names_no_branch_but_keeps_its_place`

- **WHEN** an adopted worktree is checked out at a detached HEAD
- **THEN** the project names no worktree branch
- **AND** its git branch is the short sha, and it is not marked as a group
