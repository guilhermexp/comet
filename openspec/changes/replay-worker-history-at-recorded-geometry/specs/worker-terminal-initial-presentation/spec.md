## ADDED Requirements

### Requirement: History is decoded at the grid it was produced for

The application SHALL decode recovered Worker output at the PTY grid in effect when each byte was produced, as recorded by the session host, and SHALL apply the panel's grid only after recovery completes. A session recorded before the host kept this record SHALL be decoded at an estimated grid when its log carries a row-wide rule, and at the previous grid otherwise.

#### Scenario: Differential TUI redraw in history
Test: unit — `feed_at_recorded_geometry` against a reference decode.
- **WHEN** a Worker's TUI redrew by moving up the rows its last frame occupied at its own width
- **THEN** the recovered screen matches decoding at that width and height
- **AND** no stale cell appears between rewritten words

#### Scenario: Grid changed during the session
Test: unit — mark boundary inside an output chunk.
- **WHEN** the PTY was resized while the Worker produced output
- **THEN** bytes before the change decode at the old grid and bytes after it at the new grid, regardless of how reads split the stream

#### Scenario: Stopped Worker in a narrower panel
Test: none — native GPUI visual validation.
- **WHEN** a stopped Worker's history was drawn wider than the panel showing it
- **THEN** the rows keep the recorded width and the panel scrolls horizontally
- **AND** no recorded row is split across panel rows
