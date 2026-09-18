## Purpose

Preservar a identidade de projetos e a relação entre seus checkouts durante cadastro, execução de Workers, remoção de pastas e recuperação do histórico.

## ADDED Requirements

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
The app SHALL keep launch cwd exact, distinguish archive from physical deletion, and preserve history when a checkout leaves the working set.

#### Scenario: An available external checkout launches
- **WHEN** a Worker is launched for an externally registered checkout
- **THEN** it executes in that exact checkout rather than the project principal
- **AND** the UI does not offer physical deletion as an app-owned worktree

#### Scenario: An unavailable checkout is selected
- **WHEN** a missing checkout or a project container is selected
- **THEN** launch and filesystem actions are unavailable until an available checkout is explicitly selected or restored

#### Scenario: A managed checkout is removed
- **WHEN** physical removal is requested
- **THEN** the app validates that it is an owned linked worktree, not the principal or an arbitrary directory
- **AND** active Workers and unpreserved local changes block removal
- **AND** success retains session history and does not implicitly delete the branch
- **AND** failure retains the registration and reports the error

#### Scenario: A checkout is archived
- **WHEN** the user archives a checkout
- **THEN** it leaves the active working set without losing its sessions or files
- **AND** its history remains accessible under its project
