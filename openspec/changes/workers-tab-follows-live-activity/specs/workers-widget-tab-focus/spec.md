## ADDED Requirements

### Requirement: The Workers widget tab follows live activity

The Workers widget SHALL choose its visible tab from the live state of the three lists — Workflows, Subagents, Workers — re-evaluated on every render. A tab SHALL take focus when it gains a row, when its newest start timestamp advances, or when any of its rows starts running, including a row that was already listed and had gone idle. When no tab gains anything and the tab currently in focus has nothing running while another tab does, focus SHALL move to the tab that still has running work. Ties in both cases SHALL resolve in the order Workflows, Workers, Subagents. This selection SHALL override an explicit tab click.

#### Scenario: A worker that goes back to work pulls focus
Test: UI unit test in `widgets.rs` with a worker id absent from the previous running set, unchanged count, and unchanged creation timestamp.

- **WHEN** a worker already in the list starts running again
- **THEN** the Workers tab takes focus
- **AND** it does so without its row count growing or its creation timestamp advancing

#### Scenario: Focus leaves a tab whose work finished
Test: UI unit test in `widgets.rs` parking focus on Subagents, then ending the subagent while a worker keeps running.

- **WHEN** the tab in focus has no running row and another tab does
- **THEN** focus moves to the tab that still has running work
- **AND** a tab that merely lost a row while keeping other running work stays in focus

#### Scenario: A launch outranks an emptying tab
Test: UI unit test in `widgets.rs` where a subagent ends and a workflow starts in the same sync.

- **WHEN** one tab gains work in the same sync in which another empties
- **THEN** the tab that gained work takes focus

#### Scenario: A worker launch that also mints subagents lands on the worker
Test: UI unit test in `widgets.rs` with both the worker and subagent lists growing in one sync.

- **WHEN** more than one tab gains work in the same sync
- **THEN** focus resolves in the order Workflows, Workers, Subagents

### Requirement: Opening a chat is never read as activity

The first focus sync after a chat is selected SHALL record a baseline only and SHALL NOT move the tab, so that opening a chat whose workers are already running does not pull the tab away from the reader. Switching chats SHALL discard the baseline, the running sets, and any explicit selection. A list the widget could not read SHALL be treated as absent, never as empty: it SHALL leave the baseline, the running sets, and the selection untouched, so that the list healing back does not read as a launch.

#### Scenario: Opening a chat with running workers keeps the auto order
Test: UI unit test in `widgets.rs` whose first sync already carries running rows.

- **WHEN** the first sync of a chat reports rows that are already running
- **THEN** no tab is selected by that sync
- **AND** the visible tab is the one the recency order picks

#### Scenario: An unavailable list and its recovery are not activity
Test: UI unit test in `widgets.rs` passing the workers list as absent, then restoring it unchanged.

- **WHEN** the workers list cannot be read and later returns with the same rows
- **THEN** no tab takes focus from the outage or from the recovery
- **AND** work that started during the outage still takes focus once the list returns
