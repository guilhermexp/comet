## ADDED Requirements

### Requirement: The Workers tree can be filtered to one project

The Workers sidebar SHALL offer a project filter above its scroll region,
listing the root projects of `WorkersModel::projects()` plus an "All projects"
row and a "New project…" action. Picking a project SHALL narrow the tree to
that project and its subtree; "All projects" SHALL restore the full tree.

#### Scenario: A picked project keeps its subtree

Test: `project_filter_keeps_the_subtree_of_the_picked_project`

- **WHEN** the filter names a project that has a worktree and a group under it
- **THEN** that project, its worktree and its group are the rows drawn
- **AND** projects outside that subtree are not drawn

#### Scenario: All projects restores the tree

Test: `project_filter_keeps_the_subtree_of_the_picked_project`

- **WHEN** the filter is cleared
- **THEN** every project the tree would otherwise draw is drawn

### Requirement: A stale filter reads as All projects

A filter naming a project absent from the current snapshot SHALL be treated as
"All projects" rather than drawing an empty tree.

#### Scenario: The filtered project was removed

Test: `project_filter_falls_back_when_its_project_is_gone`

- **WHEN** the snapshot no longer contains the filtered project
- **THEN** the tree draws every project
- **AND** the filter no longer names the missing project

### Requirement: The dropdown lists root projects ranked by the query

The dropdown SHALL list only projects without a parent, ranked by the search
query, and SHALL show the "All projects" row only while the query is empty.

#### Scenario: Search narrows to matching roots

Test: `project_filter_menu_rows_rank_roots_and_hide_all_while_searching`

- **WHEN** the query is empty
- **THEN** the rows are "All projects", every root project, then "New project…"
- **WHEN** the query matches one root
- **THEN** the rows are that root then "New project…", with no "All projects"
- **AND** a worktree or group is never listed

### Requirement: The filter never hides the selected project

The tree SHALL draw the selected project's root and its subtree regardless of
the filter, so a selection revealed from elsewhere always has a row.

#### Scenario: A revealed session outside the filter still has a row

Test: `project_filter_never_hides_the_selected_project`

- **WHEN** the filter names one project and the selection is a worktree of another
- **THEN** the filtered project, the selected project's root and that root's
  subtree are all drawn
