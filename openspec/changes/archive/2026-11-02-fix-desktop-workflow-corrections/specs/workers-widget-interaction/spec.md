## ADDED Requirements

### Requirement: Active Worker menu-bar rows keep stable positions
The macOS active Workers menu-bar popover SHALL order each section by Worker creation order with identifier tie-break, so activity updates never reorder a section and new Workers append. Only a real section change MAY move a row, and each row stays bound to its Worker. All recent and the Workers sidebar keep recent-activity ordering.

#### Scenario: Activity bursts do not reorder working rows
Test: unit — activity menu model fixture.
- **WHEN** three working Workers receive alternating activity updates
- **THEN** the working section lists them in creation order after every update
- **AND** each row's action opens the same Worker as before

#### Scenario: New and transitioning Workers
Test: unit — activity menu model fixture.
- **WHEN** a new Worker starts working and an existing Worker changes section
- **THEN** the new Worker appears last in its section and the transitioning Worker moves to its new section
- **AND** the remaining rows keep their relative order

#### Scenario: Recent ordering is unchanged
Test: unit — existing recent and sidebar ordering fixtures.
- **WHEN** activity updates arrive
- **THEN** All recent and the Workers sidebar still order by most recent activity
