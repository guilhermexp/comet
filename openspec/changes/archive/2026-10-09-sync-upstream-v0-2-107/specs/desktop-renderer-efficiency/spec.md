## ADDED Requirements

### Requirement: Startup retains bounded durable state without redundant rendering
The system SHALL compact redundant local-only registry history and oversized persistence without discarding unsent synchronized changes. Startup SHALL avoid loading unviewed archived Chats, and time-dependent presentation SHALL refresh at observable deadlines rather than continuously republishing unchanged state.

#### Scenario: Local registry accumulates repeated row updates
Test: unit — registry compaction, snapshot and persistence regressions.
- **WHEN** a local-only profile repeatedly updates the same row and reopens
- **THEN** current row state remains intact without retaining an unbounded unsent operation history
- **AND** an edge-connected profile preserves its durable pending sync operations

#### Scenario: A device crosses its visible offline deadline
Test: unit — desktop presentation clock regressions; none — native desktop acceptance.
- **WHEN** no data event arrives while a device reaches its offline deadline
- **THEN** visible status changes at that deadline and renewed presence schedules its next deadline
