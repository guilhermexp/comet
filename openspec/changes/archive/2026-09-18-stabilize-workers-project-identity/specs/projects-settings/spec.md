## MODIFIED Requirements

### Requirement: The project ledger outlives the working set

comet SHALL keep a project ledger that records every project the app has seen,
with checkout records keyed by canonical path and associated with a stable project identity. Archiving a checkout from the workers sidebar SHALL NOT
remove its ledger entry.

#### Scenario: A project removed from the sidebar keeps its row in settings
Test: unit — ledger reconciliation across an archive action.

- **WHEN** a project is removed through the workers sidebar context menu
- **THEN** its sessions are retained in history and its checkout is archived from the working set
- **AND** its ledger entry remains, with its path, name, added date and last-seen date intact
- **AND** Settings → Projects keeps it accessible under its owning project

#### Scenario: A project seen for the first time is recorded
Test: unit — reconciliation over a working set with no matching ledger entry.

- **WHEN** a project is present in the working set with no ledger entry
- **THEN** a ledger entry is created for its canonical path
- **AND** its added date is the moment it was first recorded

#### Scenario: Re-adding a removed folder reuses its history
Test: unit — `add_project` on a path that is ledger-only.

- **WHEN** a folder that is ledger-only is added again through the sidebar
- **THEN** it becomes live with its retained checkout identity when available, otherwise a new execution registration linked to the same historical checkout
- **AND** its added date is the original one, not the re-add moment

#### Scenario: The ledger never costs an unrelated key
Test: unit — write the ledger into a state document carrying unknown keys.

- **WHEN** the ledger is written
- **THEN** every top-level key the app does not model is still present afterwards
- **AND** the working-set project list is unchanged

#### Scenario: Organizational groups never become ledger rows
Test: unit — bootstrap projection plus duplicate-path reconciliation.

- **WHEN** the Workers bootstrap contains a filesystem project and an organizational group sharing its path
- **THEN** Settings lists only the logical filesystem project
- **AND** the persisted ledger contains exactly one entry for that canonical path
- **AND** a worktree with its own path remains eligible for the ledger

### Requirement: Settings offers a Projects section listing every recorded project

The settings navigation SHALL include a Projects section rendering a searchable
list of logical projects on the left and the selected project's detail, checkouts and history on the right. Every retained checkout SHALL remain reachable inside its project; unresolved records SHALL remain reachable in an explicit association-pending section.

#### Scenario: The list shows live and ledger-only projects together
Test: unit — row construction from a reconciled set; visual — the section.

- **WHEN** the Projects section is opened
- **THEN** each logical project is listed once, ordered by the latest real activity of its checkouts, most recent first
- **AND** each row shows the project name and when it was last opened

#### Scenario: Search filters by name and path
Test: unit — the filter predicate.

- **WHEN** the user types into the search field
- **THEN** only projects whose name or any checkout name, branch or path contains the query, case-insensitively, are listed
- **AND** a query matching nothing shows an explicit empty result, not a blank list

#### Scenario: Opening the section with no projects
Test: unit — the empty-state branch.

- **WHEN** the ledger is empty
- **THEN** the pane states that there are no projects
- **AND** offers to add one

#### Scenario: Selecting a project shows its detail
Test: visual — the two panes.

- **WHEN** the user selects a row
- **THEN** the right pane shows that project's identity and checkout list; selecting a checkout shows its General, Config, Worktree, Auto Doc and Danger Zone cards

#### Scenario: A long ledger remains navigable
Test: visual — list taller than the Projects pane.

- **WHEN** the ledger has more rows than fit vertically in the left pane
- **THEN** the list scrolls independently
- **AND** every recorded project and its checkout history remain reachable

#### Scenario: Missing worktrees do not become independent projects
- **WHEN** a project has both available and missing worktrees with known membership
- **THEN** its single project row exposes available checkouts and a separate historical section
- **AND** a missing checkout has no launch or filesystem actions

#### Scenario: Search identifies the matching checkout
- **WHEN** a query matches a child branch or historical path
- **THEN** the owning project is returned and the matching checkout is identified in its detail


### Requirement: A project can be forgotten

The Danger Zone SHALL let the user forget an inactive checkout's recorded presentation metadata, and
SHALL state that files on disk are not affected.

#### Scenario: Forgetting a project
Test: unit — the ledger delete; visual — the confirmation.

- **WHEN** the user confirms the forget action
- **THEN** the checkout's presentation metadata is forgotten and any unreferenced app-owned icon is deleted
- **AND** the checkout disappears from the settings history without hiding sibling checkouts
- **AND** no file inside the project folder is touched

#### Scenario: The confirmation names what is lost
Test: visual — the dialog.

- **WHEN** the forget action is invoked
- **THEN** a confirmation names the project and states that files on disk are kept
- **AND** cancelling leaves the ledger unchanged

#### Scenario: Forgetting does not remove sessions
Test: unit — sessions after a ledger delete.

- **WHEN** a live checkout is selected for forgetting
- **THEN** forgetting is unavailable until the checkout is archived; sessions are never deleted by this action
- **AND** polling does not reintroduce forgotten metadata; explicitly adding the checkout again restores its presence

#### Scenario: Forgotten metadata does not erase session access
- **WHEN** an inactive checkout with retained sessions has its metadata forgotten
- **THEN** those sessions remain accessible through Workers history with their minimum checkout context
