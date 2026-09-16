## ADDED Requirements

### Requirement: Task snapshots start expanded
Task snapshots in the Chat transcript SHALL initially display their items below the summary, including completed tasks. An explicit user fold choice SHALL take precedence over this default.

#### Scenario: Completed task appears in streaming
Test: none — native rendering; review with the mock demo.
- **WHEN** a completed task snapshot first appears without an explicit fold choice
- **THEN** its task text and completion status appear below the summary

#### Scenario: User collapses a task snapshot
Test: none — native disclosure interaction; review with the mock demo.
- **WHEN** the user collapses an expanded task snapshot
- **THEN** its items remain hidden on subsequent renders until expanded again
