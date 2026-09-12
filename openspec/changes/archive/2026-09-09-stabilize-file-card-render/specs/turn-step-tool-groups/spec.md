## ADDED Requirements
### Requirement: File card layout work remains bounded and preserves the current sticky turn
Collapsed file cards SHALL use only the bounded durable preview even when full input has been fetched. Reopening SHALL reuse cached full input. Measuring a card SHALL NOT continuously schedule frames and SHALL invalidate only subsequent user-row geometry when its height changes; the current turn header SHALL remain available.
#### Scenario: Collapse after loading a large file
Test: unit — ui file_change preview selection; none — native GPUI interaction and frame scheduling.
- **WHEN** a large fetched file is collapsed and reopened
- **THEN** collapsed rendering uses the bounded preview and reopening uses the cached full file
- **AND** unchanged measurements schedule no further render
#### Scenario: A file below the sticky header changes height
Test: unit — sticky geometry retention; none — native streaming review.
- **WHEN** a file card below the current user row changes height
- **THEN** its current sticky geometry remains valid and only subsequent user rows are invalidated
