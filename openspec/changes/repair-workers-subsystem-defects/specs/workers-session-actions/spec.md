## ADDED Requirements

### Requirement: A requested session mutation is never dropped

Every session mutation the user requests SHALL be dispatched, SHALL run in the
order it was requested, and SHALL keep running after an earlier one fails. No
mutation may be discarded because another one is in flight.

#### Scenario: A second command arrives during the first round-trip

Test: `action_queue_dispatches_queued_actions_in_order`

- **WHEN** a mutation is requested while another is still running
- **THEN** it is queued and dispatched when the running one settles
- **AND** two queued removals both reach the host

#### Scenario: A failure does not swallow the next command

Test: `action_queue_failure_does_not_swallow_subsequent_actions`

- **WHEN** a queued mutation fails
- **THEN** the failure is reported
- **AND** the mutations behind it still run

#### Scenario: Selecting an unread session does not eat the next command

Test: headed GPUI smoke — the automatic `mark_read` runs inside `cx.spawn`.

- **WHEN** an unread session is selected, firing the automatic read receipt
- **AND** the user immediately archives, stops or renames it
- **THEN** that command is honored

### Requirement: Archiving moves the viewer from every entry point

Archiving the session on screen SHALL move the viewer to the neighbour that
stays in the tree, whether it was requested by shortcut or by context menu, and
selection SHALL NOT survive on an archived session.

#### Scenario: The context menu archives the open session

Test: headed GPUI smoke — the native context menu has no render harness.

- **WHEN** the open session is archived from its context menu
- **THEN** the viewer moves like it does for the shortcut

#### Scenario: An archived id stops being selected

Test: `reconcile_selection_discards_archived_session`

- **WHEN** a snapshot arrives with the selected session archived
- **THEN** the selection is dropped instead of pointing at a row the tree no longer paints

#### Scenario: The neighbour is the one the user sees

Test: `selection_after_remove_orders_by_activity_rather_than_raw_host_order`

- **WHEN** the viewer hands over after an archive or a removal
- **THEN** the neighbour is chosen in the order the sidebar paints, not the order the host returns

#### Scenario: Removing an archived session keeps a selection

Test: `selection_after_remove_handles_archived_session_list`

- **WHEN** an archived session is removed from the archive drawer
- **THEN** the next archived sibling is selected instead of clearing the selection

### Requirement: The sidebar honors the project's chosen sort

The sidebar SHALL order a project's sessions by the project's `session_sort`:
`RecentlyUpdated` by newest activity, `Custom` in the order the host returns.

#### Scenario: The menu item changes the screen

Test: `project_sessions_sorted_honors_session_sort`

- **WHEN** the project's sort is switched between Custom and Recently updated
- **THEN** the painted order changes accordingly

### Requirement: The selected session is always on screen

The sidebar SHALL keep the selected session among the painted rows even when
its rank falls past the row cap, and the reveal control SHALL report the real
hidden count.

#### Scenario: Selection past the cap

Test: `project_session_row_plan_includes_selected_session_beyond_cap`

- **WHEN** the selected session ranks past the cap
- **THEN** its row is painted
- **AND** the hidden count excludes it

### Requirement: An abnormal exit does not advertise itself as a clean finish

The session indicator SHALL keep the attention marker for a session that was
waiting on input, and SHALL NOT show the unread dot for a session that died
while starting or working.

#### Scenario: A blocked session that exits

Test: `exited_workers_with_anomalous_activities_map_correctly`

- **WHEN** an unread session exits while blocked
- **THEN** it still renders the attention marker
- **AND** a session that died mid-run renders as exited, not as ready to read
