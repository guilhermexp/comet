## ADDED Requirements

### Requirement: Workers inherits the shared canvas
The Workers page SHALL use the same underlying canvas as Orchestrator without an additional default terminal fill.

#### Scenario: Selected Worker canvas
- **WHEN** a Worker is loading, live or stopped
- **THEN** its default background inherits the page canvas and explicit ANSI colors remain visible
- **Test:** unit

### Requirement: Worker resize is serialized
The UI SHALL use host-supported cell dimensions and SHALL serialize resize requests while preserving the latest measured target.

#### Scenario: Resize during an in-flight request
- **WHEN** a second measurement arrives before the first resize completes
- **THEN** no overlapping request starts and the latest geometry is applied after completion
- **Test:** unit

### Requirement: Stopped Workers retain a readable last screen
A stopped Worker SHALL retain the last alternate screen when shutdown restores an empty primary screen.

#### Scenario: Alternate screen exit on shutdown
- **WHEN** a stopped Worker journal ends by leaving a populated alternate screen for an empty primary screen
- **THEN** its final alternate screen remains readable after recovery
- **Test:** unit

#### Scenario: Live or populated primary screen
- **WHEN** a Worker is still live or its primary screen contains output
- **THEN** normal ANSI screen selection remains authoritative
- **Test:** unit

### Requirement: Panel dividers resize independent columns
Dragging the utility-panel or Details divider SHALL calculate widths from the painted columns, preserve the neighboring column outside takeover mode, and clamp at the visible layout budget without storing invisible excess width.

#### Scenario: Utility divider with Details open
- **WHEN** the utility divider moves with Details visible
- **THEN** the utility width excludes Details and the Details width remains fixed
- **Test:** unit

#### Scenario: Divider reverses at a layout limit
- **WHEN** a drag reaches the minimum main-column width and then reverses
- **THEN** the selected divider moves immediately without changing the neighboring panel first
- **Test:** unit

#### Scenario: Details divider after responsive compression
- **WHEN** Details is dragged after the window has compressed both right columns
- **THEN** the drag starts from their visible widths and preserves the utility width
- **Test:** unit

### Requirement: Terminal geometry follows the visible panel
Local terminal geometry SHALL follow every measured panel size without waiting for a host resize response. Stopped alternate-screen content SHALL reflow without losing columns when narrowed and widened.

#### Scenario: Panel moves while host resize is pending
- **WHEN** another panel measurement arrives during a host resize
- **THEN** the local grid adopts the measurement immediately and the host requests remain serialized
- **Test:** unit

#### Scenario: Stopped alternate screen is narrowed and widened
- **WHEN** a stopped alternate screen is made narrower and then restored to its initial width
- **THEN** its text remains intact and fills its original columns
- **Test:** unit

#### Scenario: Stopped history opens in a narrow panel
- **WHEN** a stopped journal contains text written with automatic wrapping disabled
- **THEN** replay preserves columns up to the supported host width before fitting the completed read-only grid into the panel
- **Test:** unit
