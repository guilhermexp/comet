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
