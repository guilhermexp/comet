# workers-repository-identity Specification

## Purpose

Preservar a identidade de projetos e a relação entre seus checkouts durante cadastro, execução de Workers, remoção de pastas e recuperação do histórico.

## Requirements

### Requirement: Repository membership survives checkout disappearance
The app SHALL persist checkout membership independently of filesystem availability, preserve existing session identifiers, and distinguish project identity from branch, remote, checkout type and directory ownership.

#### Scenario: An externally created worktree is registered and later removed
- **WHEN** a linked worktree is added and its directory is subsequently removed outside the app
- **THEN** it remains associated with the same project in history
- **AND** its sessions and last-known context remain accessible without claiming the checkout is available

#### Scenario: The main checkout is not registered
- **WHEN** Git identifies the owning repository of a registered linked worktree but its principal checkout is not registered
- **THEN** a stable project container groups the worktree
- **AND** the container is not itself an executable checkout
- **AND** registering the principal later does not create a second container

#### Scenario: Groups and remote aliases do not merge repositories
- **WHEN** an organizational group shares a path, or distinct clones share a remote or basename
- **THEN** the group is not selected as the repository identity and distinct clones are not automatically merged

#### Scenario: Local and detached checkouts remain identifiable
- **WHEN** a repository has no origin remote or a checkout has detached HEAD
- **THEN** project association still works
- **AND** detached HEAD is not treated as a branch for PR queries

### Requirement: Reconciliation is conservative and repeatable
The app SHALL reconcile legacy entries without deleting sessions, branches or directories, preserve unknown state fields and existing metadata, and expose ambiguous associations for explicit user resolution.

#### Scenario: A missing legacy path has no remaining Git evidence
- **WHEN** the path and Git worktree registration are gone and no persisted association exists
- **THEN** the record remains reachable as association pending
- **AND** no parent is guessed solely from its name, path prefix or remote

#### Scenario: Reconciliation repeats while another writer updates sessions
- **WHEN** reconciliation runs again or a session is updated during discovery
- **THEN** the session update survives, unchanged records retain their IDs and dates, and repeated reconciliation introduces no duplicate identity

#### Scenario: A path is reused by a different repository
- **WHEN** current Git evidence conflicts with the stored association
- **THEN** the conflict is visible and old sessions are not silently reassigned

#### Scenario: A user resolves an ambiguous historical association
- **WHEN** the user chooses a project after reviewing the affected checkout and sessions
- **THEN** the association persists across restart and can be undone without deleting those sessions

### Requirement: Actions respect checkout availability and ownership
The app SHALL keep launch cwd exact, distinguish archive from physical deletion, and preserve history when a checkout leaves the working set. Physical deletion SHALL require durable evidence that the app created the exact linked checkout for its current Git repository, plus a location under the canonical or a legacy worktree root. A location alone SHALL NOT prove ownership. "In use" SHALL cover a local `Working` Chat in the checkout as well as a live Worker for every removal entry point.

#### Scenario: An available external checkout launches
Test: integration — `worktree_registered_as_a_plain_project_is_projected_as_a_worktree`.

- **WHEN** a Worker is launched for an externally registered checkout
- **THEN** it executes in that exact checkout rather than the project principal
- **AND** the UI does not offer physical deletion as an app-owned worktree

#### Scenario: An unavailable checkout is selected
Test: integration — `crates/workers-unpeel/tests/project_actions.rs` unavailable checkout actions.

- **WHEN** a missing checkout or a project container is selected
- **THEN** launch and filesystem actions are unavailable until an available checkout is explicitly selected or restored

#### Scenario: A managed checkout is removed
Test: integration — `worktree_lifecycle_removes_checkout_but_retains_child_history`.

- **WHEN** physical removal is requested
- **THEN** the app validates that it is an owned linked worktree, not the principal or an arbitrary directory
- **AND** active Workers, a local `Working` Chat in the same checkout, and local changes left after any cleanup hook block removal
- **AND** success retains session history and preserves the local branch
- **AND** failure retains the registration and reports the error

#### Scenario: An external checkout under the app root remains external
Test: integration — `project_actions` with `git worktree add` under the canonical root.

- **WHEN** an externally created worktree under the canonical root is associated with a project
- **THEN** it can launch a Worker in that exact path
- **AND** physical removal is refused without changing its checkout or branch

#### Scenario: A Worker checkout created in the canonical root is removable
Test: integration — `crates/workers-unpeel/tests/project_actions.rs` lifecycle test with the worktree root overridden.

- **WHEN** a clean worktree the app created for a Worker under the canonical root is removed
- **THEN** the checkout is removed and its project history is retained

#### Scenario: A checkout is archived
Test: integration — `archiving_and_restoring_a_checkout_preserves_session_artifacts_and_ids`.

- **WHEN** the user archives a checkout
- **THEN** it leaves the active working set without losing its sessions or files
- **AND** its history remains accessible under its project

### Requirement: A worktree never projects as a group

The projection SHALL decide, once per project, whether it is an organizational
group — a folder with a parent and no worktree branch in the registry — and
SHALL NOT apply disk detection to a record that is a group. Values already
present in the registry SHALL still win: detection only fills what is absent. A
worktree therefore SHALL reach the UI as a project that owns a checkout:
selectable, launchable, and a member of the project ledger. The group verdict,
not the presence of a parent, SHALL decide whether the project menu offers
removing a group or acting on a checkout, and which confirmation the sidebar
draws.

#### Scenario: A registered worktree is not a group

Test: `worktree_lifecycle_removes_checkout_but_retains_child_history`

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

#### Scenario: A child that is not a group is not removed as a group

Test: `adopted_worktree_without_a_branch_removes_as_a_project` in
`crates/ui/src/workers/project_menu.rs`, and
`a_child_that_is_not_a_group_removes_as_a_project` for the route the item
dispatches.

- **WHEN** the menu is built for a child project that carries no worktree
  branch and is not a group
- **THEN** the menu offers the checkout action — archive, since the app did
  not create it — and never "Remove group"

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

Test: `worktree_registered_as_a_plain_project_is_projected_as_a_worktree`

- **WHEN** a session is launched into a worktree whose identity came from disk,
  with the path and branch the sidebar read from the snapshot
- **THEN** the request is not refused as "worktree does not belong to project"

#### Scenario: A failed launch keeps the checkout it created

Test: `a_failed_launch_keeps_the_worktree_it_just_created`

- **WHEN** creating a worktree succeeds and the launch that follows fails
- **THEN** the error is reported
- **AND** the checkout stays on disk, as it already does for a failed setup

### Requirement: Directory identity survives device renumbering
On macOS the app SHALL preserve a repository identity across changes to the operating system's temporary device number when the volume and Git common directory remain the same. Replacement of that directory or volume MUST remain detectable.

#### Scenario: The same directory is observed after a device number change
- Test: integration — `checkout_identity_recovery` validates stable identity observation and reconciliation
- **WHEN** the same volume and Git common directory are observed with a different device number
- **THEN** repository and checkout IDs remain unchanged and no new identity conflict is introduced

#### Scenario: A different directory occupies the registered path
- Test: integration — `checkout_identity_recovery` and existing `project_identity_edges`
- **WHEN** a different Git common directory or volume occupies a registered canonical path
- **THEN** the old association remains blocked and sessions are not reassigned

### Requirement: Obsolete fingerprints have explicit guarded recovery
The app SHALL offer diagnosis and explicit revalidation of the selected repository identity using fresh Git and filesystem evidence. Recovery MUST require the caller's expected old and observed fingerprints, reject concurrent changes, preserve unrelated state and IDs, and clear only the fingerprint conflict being repaired. Ambiguous legacy identities SHALL NOT be accepted automatically merely because their inodes match.

#### Scenario: The owner revalidates an obsolete fingerprint
- Test: integration — `checkout_identity_recovery` with a private state file and disposable Git repositories
- **WHEN** the caller confirms the expected old and current fingerprints for an available repository
- **THEN** the stable fingerprint is persisted and matching primary and linked checkouts can launch in their exact paths
- **AND** sessions, presets and unrelated fields remain unchanged

#### Scenario: State or filesystem changes after diagnosis
- Test: integration — `checkout_identity_recovery` exercises stale expectations and unrelated conflicts
- **WHEN** the stored identity or observed directory differs from the values supplied for recovery
- **THEN** recovery fails without altering state or clearing unrelated conflicts

### Requirement: Known blocked projects report their launch blocker
Every launch surface SHALL distinguish an unknown project from a registered checkout blocked by conflict, unavailable filesystem, archive, removal, or unreadable identity metadata. Failure MUST occur before spawning a worker and MUST retain the exact requested checkout.

#### Scenario: A listed project has an identity conflict
- Test: integration — `checkout_identity_recovery` through controller MCP and the real Host route
- **WHEN** launch is requested for a registered checkout with an identity conflict
- **THEN** the error identifies the conflict and recovery action rather than saying the project is unknown
- **AND** no worker process or session is created

#### Scenario: The requested project does not exist
- Test: integration — `checkout_identity_recovery`
- **WHEN** launch names an unregistered ID
- **THEN** the error identifies the unknown project without adopting another checkout
