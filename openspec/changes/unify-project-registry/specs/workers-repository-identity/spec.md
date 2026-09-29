# Spec Delta

## MODIFIED Requirements

### Requirement: Repository membership survives checkout disappearance
The app SHALL persist checkout membership independently of filesystem availability, preserve existing session identifiers, and distinguish project identity from branch, remote, checkout type and directory ownership. A repository's project SHALL be the Space of its repository root on the local device.

#### Scenario: An externally created worktree is registered and later removed
- Test: integration — `worktree_lifecycle_removes_checkout_but_retains_child_history`.
- **WHEN** a linked worktree is added and its directory is subsequently removed outside the app
- **THEN** it remains associated with the same project in history
- **AND** its sessions and last-known context remain accessible without claiming the checkout is available

#### Scenario: The main checkout is not registered
- Test: integration — register only a linked worktree, then list projects; register the principal later.
- **WHEN** Git identifies the owning repository of a registered linked worktree but its principal checkout has no execution registration
- **THEN** the worktree is a checkout of the project for the repository root, named after that project
- **AND** launching the project runs in the repository root without requiring a prior principal registration
- **AND** registering the principal later does not create a second project

#### Scenario: Groups and remote aliases do not merge repositories
- Test: unit — identity clustering over a group sharing a path and two clones sharing a remote.
- **WHEN** an organizational group shares a path, or distinct clones share a remote or basename
- **THEN** the group is not selected as the repository identity and distinct clones are not automatically merged

#### Scenario: Local and detached checkouts remain identifiable
- Test: unit — identity over a repository with no origin and a detached checkout.
- **WHEN** a repository has no origin remote or a checkout has detached HEAD
- **THEN** project association still works
- **AND** detached HEAD is not treated as a branch for PR queries
