## MODIFIED Requirements

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
