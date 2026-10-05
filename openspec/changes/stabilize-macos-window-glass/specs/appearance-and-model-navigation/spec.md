## ADDED Requirements

### Requirement: Desktop glass remains stable during repaint

On macOS, the desktop app SHALL preserve its configured glass tint and blur while content repaints. Surface reapplication SHALL NOT accumulate native backing surfaces, and switching to opaque mode SHALL remove glass support from the window.

#### Scenario: Content repaints over declared-radius glass

Test: integration — native macOS backing configuration and lifecycle; none — moving-wave appearance requires headed visual comparison over the same desktop, and automated lifecycle assertions alone do not prove its disappearance.

- **WHEN** a glass theme with an authored blur radius is displayed and content repaints
- **THEN** native composition support remains present behind the content
- **AND** the configured tint and blur radius remain unchanged

#### Scenario: Glass is reapplied or replaced

Test: integration — native macOS backing reuse and removal through surface transitions.

- **WHEN** the same glass surface is reapplied, its blur radius changes, or it switches to opaque or platform-material mode
- **THEN** reapplication reuses the existing supporting surface
- **AND** leaving declared-radius glass removes that surface without changing the selected theme or leaving stale glass behind
