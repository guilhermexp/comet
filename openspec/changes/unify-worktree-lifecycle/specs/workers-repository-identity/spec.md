## MODIFIED Requirements

### Requirement: Actions respect checkout availability and ownership
The app SHALL keep launch cwd exact, distinguish archive from physical deletion, and preserve history when a checkout leaves the working set. Ownership for physical deletion SHALL recognise the canonical worktree root and the legacy ones alike, and "in use" SHALL cover a `Working` Chat in the checkout as well as a live Worker.

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
- **AND** active Workers, a `Working` Chat in the same checkout, and unpreserved local changes block removal
- **AND** success retains session history and does not implicitly delete the branch
- **AND** failure retains the registration and reports the error

#### Scenario: A Worker checkout created in the canonical root is removable
Test: integration — `crates/workers-unpeel/tests/project_actions.rs` lifecycle test with the worktree root overridden.

- **WHEN** a clean worktree the app created for a Worker under the canonical root is removed
- **THEN** the checkout is removed and its project history is retained

#### Scenario: A checkout is archived
- **WHEN** the user archives a checkout
- **THEN** it leaves the active working set without losing its sessions or files
- **AND** its history remains accessible under its project
