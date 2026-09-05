## ADDED Requirements

### Requirement: ⌘W closes the Worker session on screen

The Workers surface SHALL archive the selected session on `⌘W`, SHALL stop a
live session before archiving it, and SHALL leave the Orchestrator's own
archive verb and combo untouched.

#### Scenario: The open session leaves the tree

Test: headed GPUI smoke — action dispatch and archive round-trip have no render harness.

- **WHEN** a Worker session is open and the shortcut fires
- **THEN** the session is archived and leaves the sidebar tree
- **AND** it becomes reachable in the project's archive
- **AND** a live session is stopped before it is archived

#### Scenario: The shortcut is scoped to Workers

Test: headed GPUI smoke — action dispatch and archive round-trip have no render harness.

- **WHEN** the shortcut fires while the Orchestrator sidebar is active
- **THEN** no Chat is archived
- **AND** the Orchestrator archive shortcut keeps archiving the selected Chat
- **AND** an open picker or palette suppresses the shortcut

### Requirement: The viewer moves to a session that stays visible

After archiving, the Workers viewer SHALL show the nearest remaining
non-archived session of the same project, SHALL fall back to the preceding one
when the archived session was the last, and SHALL show no session when the
project has none left.

#### Scenario: The neighbour takes over

Test: `archiving_hands_the_viewer_to_a_session_that_stays_visible`

- **WHEN** the archived session had siblings in the project
- **THEN** the viewer shows a sibling that is still in the tree
- **AND** an already archived sibling is never selected

#### Scenario: The last session empties the viewer

Test: `archiving_the_last_session_of_a_project_empties_the_viewer`

- **WHEN** the archived session was the only one in its project
- **THEN** the viewer shows no session
- **AND** a session from another project is not pulled in
