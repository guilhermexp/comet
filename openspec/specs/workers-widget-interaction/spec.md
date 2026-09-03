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
