## ADDED Requirements

### Requirement: Sessions inside a project are ordered by activity

The Workers sidebar SHALL order the sessions of a project by most recent
activity, newest first, and SHALL keep that order total so equal timestamps
never reshuffle between frames.

#### Scenario: Newest activity leads the project

Test: `sessions_sorted_by_activity_newest_first`

- **WHEN** a project's sessions carry different `updated_at_unix_ms`
- **THEN** the most recently updated session is the first row
- **AND** the remaining rows descend by `updated_at_unix_ms`

#### Scenario: Equal activity resolves deterministically

Test: `sessions_with_equal_activity_break_ties_deterministically`

- **WHEN** two sessions share `updated_at_unix_ms`
- **THEN** the more recently created session comes first
- **AND** sessions sharing both timestamps are ordered by ascending id
- **AND** re-running the projection on the same input yields the same order

### Requirement: Projects are ordered by the activity of their subtree

The Workers sidebar SHALL order projects by the most recent session activity
within the project and its descendants, newest first, SHALL render every child
project directly beneath its parent, and SHALL place projects with no activity
after every project that has activity.

#### Scenario: A project rises with its newest session

Test: `projects_sorted_by_newest_session_activity`

- **WHEN** one project's newest session is hours old and another's is days old
- **THEN** the project with the newer session renders above the other

#### Scenario: A group carries the activity of its children

Test: `group_inherits_descendant_activity`

- **WHEN** a group holds no sessions of its own but a child project inside it has the newest activity in the tree
- **THEN** the group renders above projects whose own sessions are older
- **AND** the child project still renders directly beneath its group

#### Scenario: Projects without sessions sink and keep host order

Test: `projects_without_activity_keep_host_order_last`

- **WHEN** projects have no sessions at all
- **THEN** they render after every project that has activity
- **AND** they preserve the relative order the host returned

#### Scenario: Archived sessions do not rank a project

Test: `archived_sessions_do_not_rank_projects`

- **WHEN** a project's only recent session is archived
- **THEN** that session does not contribute to the project's rank

### Requirement: Session rows are capped behind a reveal control

The Workers sidebar SHALL render at most five session rows for an expanded
project, SHALL offer a control that reveals the remaining sessions, SHALL offer
a control that returns to the capped list, and SHALL cap the list again after
the project collapses.

#### Scenario: A long list renders capped with a reveal count

Test: `session_rows_cap_at_five_with_remaining_count`

- **WHEN** an expanded project has twelve non-archived sessions
- **THEN** five session rows render
- **AND** a reveal control reports the seven remaining sessions

#### Scenario: Revealing shows the whole list

Test: `revealing_a_project_shows_every_session`

- **WHEN** the reveal control is used on that project
- **THEN** all twelve session rows render
- **AND** a control to return to the capped list is offered

#### Scenario: Short lists carry no control

Test: `projects_at_or_below_the_cap_have_no_reveal_control`

- **WHEN** an expanded project has five or fewer non-archived sessions
- **THEN** every session renders
- **AND** no reveal or collapse control is offered

#### Scenario: Collapsing a project re-caps it

Test: `collapsing_a_project_clears_its_reveal_state`

- **WHEN** a revealed project is collapsed and expanded again
- **THEN** five session rows render
- **AND** the reveal control returns

### Requirement: A finished but unopened session keeps a blue unread dot

The Workers sidebar SHALL render the unread indicator for a session whose
process has exited while its output has never been opened, SHALL paint that
indicator blue independently of the configured accent, and SHALL let a pending
relaunch outrank it.

#### Scenario: An exited session that was never opened stays marked

Test: `finished_but_unseen_workers_keep_the_unread_indicator`

- **WHEN** a session reports an exited state and is still unread
- **THEN** the row renders the unread indicator
- **AND** the same session renders as a quiet finished row once it has been seen

#### Scenario: Relaunch outranks the dot

Test: `finished_but_unseen_workers_keep_the_unread_indicator`

- **WHEN** an unread exited session has a relaunch in flight
- **THEN** the row renders the restarting indicator instead

### Requirement: The sidebar shows only projects with something in them

The Workers sidebar SHALL omit a project that owns no session, and SHALL NOT
change the durable project ledger when it does. A project SHALL keep its row
while its subtree owns a live session, while it is the selected or launcher
project, or while it still owns archived sessions.

#### Scenario: An empty project leaves the working set

Test: `empty_projects_leave_the_sidebar_working_set`

- **WHEN** a project owns no live session
- **THEN** it is not rendered in the sidebar
- **AND** it remains in the project ledger

#### Scenario: A group survives for the session nested under it

Test: `a_group_stays_while_its_subtree_owns_a_session`

- **WHEN** a group owns no session of its own but a descendant does
- **THEN** the group still renders

#### Scenario: An archive keeps the door open

Test: `a_project_that_still_owns_an_archive_keeps_its_row`

- **WHEN** every session of a project is archived
- **THEN** the row stays, because it is the only way to reach the archive
- **AND** a project with no session and no archive is dropped

#### Scenario: A project just added stays reachable

Test: `a_freshly_added_project_stays_until_it_owns_a_session`

- **WHEN** a project is added or aimed at by the launcher
- **THEN** it renders while still empty
