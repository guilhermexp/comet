# workers-widget-interaction Specification

## Purpose

Collapsed-by-default disclosure state, activity identity binding, and lifecycle indicators for subagent and worker rows in the Details Workers widget.

## Requirements

### Requirement: Activities disclose only on user request

The Workers widget SHALL initialize every workflow and subagent row collapsed
and SHALL preserve explicit disclosure state by stable activity id.

#### Scenario: New and reordered subagents remain collapsed

Test: `workers_widget_keeps_expansion_bound_to_identity_after_reordering`

- **WHEN** new activity ids arrive or reorder during streaming
- **THEN** every unseen id is collapsed
- **AND** an explicitly expanded id retains its state
- **AND** changing chats resets the local disclosure state

### Requirement: Running subagents show lifecycle activity

Every subagent row SHALL show the existing semantic lifecycle status alongside
its avatar and title.

#### Scenario: A subagent is running

Test: headed GPUI smoke.

- **WHEN** a subagent status is `Running`
- **THEN** its row shows the shared animated spinner
- **AND** the spinner does not change row geometry
- **AND** settled rows show their semantic terminal status

### Requirement: Worker rows show launch age beside usage
The Workers widget SHALL show compact time since launch beside each Worker's token total, or beside its command fallback when tokens are unavailable. The age SHALL use the original creation time and the sidebar's relative-time units, remain readable in a narrow card and update while the widget is visible.

#### Scenario: Heartbeats and completion preserve launch age
Test: unit — timestamp projection; none — native gpui layout acceptance.
- **WHEN** a Worker receives output, heartbeat or terminal status updates
- **THEN** its displayed launch age remains derived from its creation time
- **AND** it does not reset when the Worker settles

#### Scenario: Missing usage or timestamp
Test: unit — absent and future timestamp handling; none — native gpui layout acceptance.
- **WHEN** token usage is missing
- **THEN** the command fallback and available launch age remain visible
- **AND** absent timestamps do not produce an invented age
- **AND** future timestamps clamp to the sidebar's current-time label

### Requirement: Subagent summaries distinguish running and finished work
The existing Workers widget SHALL keep running subagents visible and group settled subagents under a collapsed finished disclosure with independent Completed, Failed and Cancelled lists. Visible Chat/subagent counters SHALL derive from current lifecycle evidence and treat stale or absent evidence as zero, without replacing Worker launch age or primary-provider telemetry.

#### Scenario: Subagents settle while another is running
Test: unit — grouping, paging and running count projection; none — native widget acceptance.
- **WHEN** some subagents complete or fail while another remains running
- **THEN** the running row stays visible and finished rows move into their respective collapsed paginated lists
- **AND** running counts reflect only current running evidence

#### Scenario: A settled subagent has been cancelled
Test: unit — lifecycle correlation and worker projection.
- **WHEN** the recorded subagent lifecycle is Cancelled and its launch record has no cancellation state
- **THEN** it stays in Cancelled and does not increase the running count

### Requirement: Active Worker menu-bar rows keep stable positions
The macOS active Workers menu-bar popover SHALL keep each Worker at the position it got when the popover first saw it (Workers first seen together by creation time, then identifier), so activity never reorders a section and a newly seen Worker appends. Only a real section change MAY move a row, and each row stays bound to its Worker. All recent and the Workers sidebar keep their own recent-activity ordering.

#### Scenario: Activity bursts do not reorder working rows
Test: unit — activity menu model fixture.
- **WHEN** three working Workers receive alternating activity updates
- **THEN** the working section lists them in the order the popover first saw them after every update
- **AND** each row's action opens the same Worker as before

#### Scenario: New and transitioning Workers
Test: unit — activity menu model fixture.
- **WHEN** a new Worker starts working and an existing Worker changes section
- **THEN** the new Worker appears last in its section and the transitioning Worker moves to its new section
- **AND** the remaining rows keep their relative order

#### Scenario: Recent ordering is unchanged
Test: unit — activity menu, recent and sidebar ordering fixtures.
- **WHEN** activity updates arrive for two active Workers whose creation and activity orders disagree
- **THEN** All recent's Active section and the Workers sidebar still order by most recent activity
- **AND** the menu-bar popover keeps its own order

#### Scenario: Equal-time new Worker appends
Test: unit — activity menu model fixture; native — macOS menu-bar fixture with the production order state (run by the orchestrator).
- **WHEN** a new Worker is created at the same instant as existing Workers and its identifier sorts before theirs
- **THEN** it appears last in its section
- **AND** the existing rows keep their identities and positions through later activity bursts and reordered snapshots
