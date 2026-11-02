## MODIFIED Requirements

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
