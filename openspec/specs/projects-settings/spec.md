# projects-settings Specification

## Purpose

Project ledger persistence, Git repository metadata, worktree configuration, and settings management for registered projects.

## Requirements

### Requirement: The project ledger outlives the working set

comet SHALL keep a project ledger that records every project the app has seen,
keyed by canonical path. Removing a project from the workers sidebar SHALL NOT
remove its ledger entry.

#### Scenario: A project removed from the sidebar keeps its row in settings
Test: unit — ledger reconciliation across a `remove_project` call.

- **WHEN** a project is removed through the workers sidebar context menu
- **THEN** its sessions and its working-set record are gone
- **AND** its ledger entry remains, with its path, name, added date and last-seen date intact
- **AND** Settings → Projects still lists it

#### Scenario: A project seen for the first time is recorded
Test: unit — reconciliation over a working set with no matching ledger entry.

- **WHEN** a project is present in the working set with no ledger entry
- **THEN** a ledger entry is created for its canonical path
- **AND** its added date is the moment it was first recorded

#### Scenario: Re-adding a removed folder reuses its history
Test: unit — `add_project` on a path that is ledger-only.

- **WHEN** a folder that is ledger-only is added again through the sidebar
- **THEN** it becomes live under a new project id
- **AND** its added date is the original one, not the re-add moment

#### Scenario: The ledger never costs an unrelated key
Test: unit — write the ledger into a state document carrying unknown keys.

- **WHEN** the ledger is written
- **THEN** every top-level key the app does not model is still present afterwards
- **AND** the working-set project list is unchanged

#### Scenario: Organizational groups never become ledger rows
Test: unit — bootstrap projection plus duplicate-path reconciliation.

- **WHEN** the Workers bootstrap contains a filesystem project and an organizational group sharing its path
- **THEN** Settings lists only the filesystem project
- **AND** the persisted ledger contains exactly one entry for that canonical path
- **AND** a worktree with its own path remains eligible for the ledger

### Requirement: Settings offers a Projects section listing every recorded project

The settings navigation SHALL include a Projects section rendering a searchable
list on the left and the selected project's detail on the right.

#### Scenario: The list shows live and ledger-only projects together
Test: unit — row construction from a reconciled set; visual — the section.

- **WHEN** the Projects section is opened
- **THEN** every ledger entry is listed, ordered by last activity, most recent first
- **AND** each row shows the project name and when it was last opened

#### Scenario: Search filters by name and path
Test: unit — the filter predicate.

- **WHEN** the user types into the search field
- **THEN** only projects whose name or path contains the query, case-insensitively, are listed
- **AND** a query matching nothing shows an explicit empty result, not a blank list

#### Scenario: Opening the section with no projects
Test: unit — the empty-state branch.

- **WHEN** the ledger is empty
- **THEN** the pane states that there are no projects
- **AND** offers to add one

#### Scenario: Selecting a project shows its detail
Test: visual — the two panes.

- **WHEN** the user selects a row
- **THEN** the right pane shows that project's General, Config, Worktree, Auto Doc and Danger Zone cards

#### Scenario: A long ledger remains navigable
Test: visual — list taller than the Projects pane.

- **WHEN** the ledger has more rows than fit vertically in the left pane
- **THEN** the list scrolls independently
- **AND** every recorded project remains reachable

### Requirement: The General card shows a project's identity and lifecycle

The General card SHALL show name, icon, path, added date and last-opened date,
and SHALL let the user rename the project and set or reset its icon.

#### Scenario: Renaming a live project
Test: unit — the rename decision; visual — the field.

- **WHEN** the user edits the name field and leaves it
- **THEN** the new name is persisted for that project
- **AND** an empty or unchanged value leaves the previous name in place

#### Scenario: Setting a custom icon
Test: visual — the icon slot before and after.

- **WHEN** the user picks an image file for a project
- **THEN** that image is stored for the project and shown in the icon slot
- **AND** a reset action is offered that restores the default icon

#### Scenario: Resetting or forgetting cleans up a managed icon
Test: unit — digest filename and managed-path cleanup; visual — icon fallback.

- **WHEN** a project icon is reset or its ledger row is forgotten
- **THEN** the metadata stops naming the icon
- **AND** the exact app-owned icon file is deleted
- **AND** a path outside the app-owned icon directory is never deleted

#### Scenario: A project with no custom icon
Test: unit — the icon-source decision.

- **WHEN** a project has no stored icon
- **THEN** a folder icon is shown
- **AND** no image file is read

#### Scenario: Revealing the project folder
Test: visual — the Finder window that opens.

- **WHEN** the user activates the reveal action on the path row
- **THEN** the project folder is revealed in the system file browser

#### Scenario: Last opened for a project that left the working set
Test: unit — the last-opened source decision.

- **WHEN** a ledger-only project is selected
- **THEN** the last-opened row shows the date frozen when it left the working set
- **AND** no session is consulted

### Requirement: The General card reports the project's git state

The card SHALL show, for the selected project, whether its folder is a git
repository, whether it has a remote, and which repository it points at — read
from the folder, not from the registry.

#### Scenario: A repository with a remote
Test: unit — remote parsing to owner/repo; visual — the row.

- **WHEN** the selected project's folder has an origin remote
- **THEN** the Repository row shows owner and repository
- **AND** offers to open it in the browser when the remote is a known host
- **AND** the browser URL preserves that remote's host rather than forcing github.com

#### Scenario: A local repository with no remote
Test: unit — the three-state decision.

- **WHEN** the folder is a repository with no remote configured
- **THEN** the row says it is local and unpublished
- **AND** offers to publish it, asking for public or private before doing anything

#### Scenario: A folder that is not a repository
Test: unit — the three-state decision.

- **WHEN** the folder is not a git repository
- **THEN** the row says so
- **AND** offers to initialize one

#### Scenario: The lifecycle dates are anchored to commits
Test: unit — the commit lookup over a fixture repository.

- **WHEN** the selected project's folder is a repository
- **THEN** the added row also shows the commit that was HEAD at that date
- **AND** the last-opened row shows the commit that was HEAD at that date
- **AND** a date before the first commit shows no commit rather than an error

#### Scenario: Git failures never blank the card
Test: unit — each git reader against a non-repository and a missing folder.

- **WHEN** any git read fails or the folder no longer exists
- **THEN** the card still renders with the fields it could resolve
- **AND** the failure is visible on the affected row rather than silent

### Requirement: A project carries worktree setup commands, and they run

The Config and Worktree cards SHALL read and write the project's worktree setup
configuration, and comet SHALL execute those commands after creating a worktree
for that project.

#### Scenario: Choosing where the config is stored
Test: unit — the detection order.

- **WHEN** a project has no worktree config
- **THEN** comet's own config path is offered as the target
- **AND** the Cursor-compatible path is offered as well only when that file already exists

#### Scenario: An existing config is detected and shown
Test: unit — detection over each supported path.

- **WHEN** a project already has a worktree config at a supported path
- **THEN** its commands are shown in the Worktree card
- **AND** the Config card names the path the commands were read from

#### Scenario: Editing commands persists them
Test: unit — the save decision; visual — reopening the page.

- **WHEN** the user edits, adds or removes a setup command and leaves the field
- **THEN** the config file is written with the non-empty commands
- **AND** an edit that changes nothing writes no file

#### Scenario: Platform-specific overrides
Test: unit — round-trip of a config carrying platform lists.

- **WHEN** a project defines commands for macOS/Linux or for Windows
- **THEN** those lists are shown in their own groups
- **AND** a group left empty is stated to fall back to the shared commands

#### Scenario: The config editor writes the selected supported target
Test: unit — editor normalization and save decision; visual — target and command controls.

- **WHEN** the user changes shared, Unix or Windows commands or selects an available config target
- **THEN** non-empty non-comment commands are persisted to that target
- **AND** unchanged editor state performs no write
- **AND** the Cursor target is selectable only when its file already exists

#### Scenario: Setup runs after a worktree is created
Test: unit — the executor over a temporary checkout.

- **WHEN** a worktree is created for a project that has setup commands
- **THEN** each command runs in the new worktree
- **AND** the main checkout's path is available to those commands through the environment
- **AND** a command that fails stops the run and reports which command failed

#### Scenario: Verbose and timed-out setup commands terminate cleanly
Test: unit — bounded stderr plus descendant cleanup.

- **WHEN** a setup command fills stderr or outlives its deadline with descendants
- **THEN** stderr is drained without deadlocking the child
- **AND** the failure reports a bounded reason
- **AND** timeout terminates the command's process group before returning

#### Scenario: Setup child signals do not retire the hook ingress
Test: unit — retry classification for listener accept errors; full crate suite in parallel.

- **WHEN** setup child processes exit while the session hook listener is accepting connections
- **THEN** an interrupted accept is retried like a would-block accept
- **AND** the ingress remains available for the next hook request

#### Scenario: A project with no setup config
Test: unit — the executor's no-config branch.

- **WHEN** a worktree is created for a project with no setup commands
- **THEN** the worktree is created and nothing is executed
- **AND** no error is reported

### Requirement: The page can hand work to a worker

The Worktree and Auto Doc cards SHALL be able to start a worker session in the
selected project, seeded with a prompt.

#### Scenario: Filling the worktree setup with an agent
Test: visual — the session that appears.

- **WHEN** the user asks for the worktree setup to be filled by an agent
- **THEN** a worker session starts in that project, seeded with the request
- **AND** no config file is written by the button itself

#### Scenario: Running an Auto Doc pass
Test: visual — the session that appears.

- **WHEN** the user runs Auto Doc on a project whose folder is a repository
- **THEN** a worker session starts in that project, seeded with the audit request and the project's two anchor commits
- **AND** the action is unavailable for a project whose folder is not a repository

#### Scenario: Failed worktree setup blocks automatic worker launch
Test: unit — setup-result launch guard; visual — error notice.

- **WHEN** worktree creation succeeds but one setup command fails
- **THEN** the worktree remains registered for inspection
- **AND** the UI names the failed command and reason
- **AND** no Worker session is launched automatically in that worktree

### Requirement: A project can be forgotten

The Danger Zone SHALL let the user delete a project's recorded metadata, and
SHALL state that files on disk are not affected.

#### Scenario: Forgetting a project
Test: unit — the ledger delete; visual — the confirmation.

- **WHEN** the user confirms the forget action
- **THEN** the project's ledger entry and stored icon are deleted
- **AND** the project disappears from the settings list
- **AND** no file inside the project folder is touched

#### Scenario: The confirmation names what is lost
Test: visual — the dialog.

- **WHEN** the forget action is invoked
- **THEN** a confirmation names the project and states that files on disk are kept
- **AND** cancelling leaves the ledger unchanged

#### Scenario: Forgetting does not remove sessions
Test: unit — sessions after a ledger delete.

- **WHEN** a live project is forgotten
- **THEN** its sessions and its working-set record are untouched
- **AND** it reappears in the settings list as first seen again
