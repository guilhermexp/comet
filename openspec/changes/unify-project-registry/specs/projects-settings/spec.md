# Spec Delta

## MODIFIED Requirements

### Requirement: The project ledger outlives the working set

comet SHALL keep, for every local project, a checkout history that records every checkout the app has seen for that project,
with checkout records keyed by canonical path and linked to the project's Space id. Archiving a checkout from the workers sidebar SHALL NOT
remove its history entry and SHALL NOT delete the project.

#### Scenario: A project removed from the sidebar keeps its row in settings
- Test: unit — ledger reconciliation across an archive action.

- **WHEN** a checkout is removed through the workers sidebar context menu
- **THEN** its sessions are retained in history and its checkout is archived from the working set
- **AND** its history entry remains, with its path, name, added date and last-seen date intact
- **AND** its project remains in the registry and Settings → Projects keeps the checkout accessible under that project

#### Scenario: A project seen for the first time is recorded
- Test: unit — reconciliation over a working set with no matching ledger entry.

- **WHEN** a checkout is present in the working set with no history entry
- **THEN** a history entry is created for its canonical path under its project
- **AND** its added date is the moment it was first recorded

#### Scenario: Re-adding a removed folder reuses its history
- Test: unit — `add_project` on a path that is ledger-only.

- **WHEN** a folder that is history-only is added again through any add-project entry point
- **THEN** it becomes live under the same project with its retained checkout identity when available, otherwise a new execution registration linked to the same historical checkout
- **AND** its added date is the original one, not the re-add moment

#### Scenario: The ledger never costs an unrelated key
- Test: unit — write the ledger into a state document carrying unknown keys.

- **WHEN** the history is written
- **THEN** every top-level key the app does not model is still present afterwards
- **AND** the working-set checkout list is unchanged

#### Scenario: Organizational groups never become ledger rows
- Test: unit — bootstrap projection plus duplicate-path reconciliation.

- **WHEN** the Workers bootstrap contains a filesystem checkout and an organizational group sharing its path
- **THEN** Settings lists only the project of that checkout
- **AND** the persisted history contains exactly one entry for that canonical path
- **AND** a worktree with its own path remains eligible for the history

### Requirement: Settings offers a Projects section listing every recorded project

The settings navigation SHALL include a Projects section rendering a searchable
list of the registry's projects from every device on the left, each labelled with its device, and the selected project's detail, checkouts and history on the right. The set of listed projects SHALL equal the set returned by the chat MCP `list_projects`. Every retained checkout of a local project SHALL remain reachable inside its project; unresolved records SHALL remain reachable in an explicit association-pending section.

#### Scenario: The list shows live and ledger-only projects together
- Test: unit — row construction from a registry snapshot plus local checkout history; visual — the section.

- **WHEN** the Projects section is opened
- **THEN** each project of the registry is listed once, ordered by the latest real activity of its chats and checkouts, most recent first
- **AND** each row shows the project name, its device name and when it was last opened

#### Scenario: The list matches the chat MCP
- Test: integration — row builder and chat MCP `list_projects` over the same registry fixture.

- **WHEN** the registry holds projects on the local device and on another device
- **THEN** Settings lists exactly the project ids the chat MCP `list_projects` returns
- **AND** no row exists for a Workers registration without a project

#### Scenario: Search filters by name and path
- Test: unit — the filter predicate.

- **WHEN** the user types into the search field
- **THEN** only projects whose name, device name or any checkout name, branch or path contains the query, case-insensitively, are listed
- **AND** a query matching nothing shows an explicit empty result, not a blank list

#### Scenario: Opening the section with no projects
- Test: unit — the empty-state branch.

- **WHEN** the registry has no projects
- **THEN** the pane states that there are no projects
- **AND** offers to add one

#### Scenario: Selecting a project shows its detail
- Test: none — native GPUI QA: the two panes.

- **WHEN** the user selects a local project
- **THEN** the right pane shows that project's identity and checkout list; selecting a checkout shows its General, Config, Worktree, Auto Doc and Danger Zone cards

#### Scenario: A long ledger remains navigable
- Test: none — native GPUI QA: list taller than the Projects pane.

- **WHEN** the registry has more projects than fit vertically in the left pane
- **THEN** the list scrolls independently
- **AND** every project and its checkout history remain reachable

#### Scenario: Missing worktrees do not become independent projects
- Test: unit — row construction with available and missing linked checkouts of one project.
- **WHEN** a project has both available and missing worktrees with known membership
- **THEN** its single project row exposes available checkouts and a separate historical section
- **AND** a missing checkout has no launch or filesystem actions

#### Scenario: Search identifies the matching checkout
- Test: unit — the filter predicate on a child branch.
- **WHEN** a query matches a child branch or historical path
- **THEN** the owning project is returned and the matching checkout is identified in its detail

## ADDED Requirements

### Requirement: Remote projects are shown without local actions

A project owned by another device SHALL show its name, device, path, Git flag and creation date, and SHALL allow renaming. It SHALL NOT offer checkouts, config, worktree setup, Auto Doc, reveal, forget or any action that reads or writes its folder.

#### Scenario: Selecting a remote project
- Test: unit — card availability by device; visual — the detail pane.
- **WHEN** the user selects a project owned by another device
- **THEN** the detail shows its identity and device
- **AND** no filesystem or Worker action is offered

#### Scenario: Renaming a remote project
- Test: integration — rename through Settings on a remote Space, then chat MCP `list_projects`.
- **WHEN** the user renames a remote project
- **THEN** the chat MCP `list_projects` returns the new name

### Requirement: A project lists its sessions

The detail of a project SHALL offer a Sessions tab listing, in one list ordered by most recent activity, the project's Orchestrator chats (active and archived) and, for a local project, every Worker session launched in any of its checkouts (active and archived). Each row SHALL show its kind (chat or Worker), title, runtime, status and last activity; a Worker row SHALL also show its checkout (branch or principal) and the chat that launched it when one is recorded. Activating a row SHALL open that session in a side panel next to the list, without leaving Settings → Projects: a Worker opens the same Worker surface (terminal and transcript) used when a Worker is opened from a chat; a chat opens its transcript with an action to go to the chat. A project owned by another device SHALL list only its chats.

#### Scenario: Chats and Worker sessions in one list
- Test: unit — session rows built from a project with chats, a principal and a linked worktree with Worker sessions, plus sessions of another project.
- **WHEN** the user opens the Sessions tab of a local project
- **THEN** the project's chats and the Worker sessions of the principal and every linked checkout are listed together, most recent first, each Worker naming its checkout
- **AND** chats and sessions of other projects are absent

#### Scenario: Opening a Worker session
- Test: unit — row activation resolves to the Worker surface for that session id; none — native GPUI QA for the panel beside the list.
- **WHEN** the user activates a running or idle Worker row
- **THEN** a side panel opens next to the list with that Worker's terminal and transcript
- **AND** Settings → Projects stays open with the row selected

#### Scenario: Opening an archived Worker session
- Test: unit — activation of an archived row resolves to a read-only replay; none — native GPUI QA for the replayed transcript.
- **WHEN** the user activates an archived Worker row
- **THEN** the side panel shows that session's recorded transcript without restarting it

#### Scenario: Opening a chat
- Test: unit — activation of a chat row resolves to that chat's transcript surface.
- **WHEN** the user activates a chat row
- **THEN** the side panel shows the chat's transcript
- **AND** an action in the panel navigates to the chat

#### Scenario: Session launched from an Orchestrator chat
- Test: unit — row built from a Worker parent link.
- **WHEN** a Worker session was launched by an Orchestrator chat
- **THEN** its row names that chat and activating the chat name opens that chat in the side panel

#### Scenario: A checkout that disappeared
- Test: unit — rows for sessions of a missing checkout.
- **WHEN** a Worker session's checkout is no longer available
- **THEN** the session is still listed and opens its recorded transcript
- **AND** no restart is offered from the tab

#### Scenario: A remote project
- Test: unit — rows for a Space owned by another device.
- **WHEN** the user opens the Sessions tab of a project owned by another device
- **THEN** only its chats are listed
