## ADDED Requirements

### Requirement: Workers rows are identified by their action
A Workers tool row SHALL take its leading icon from the call's `action`
argument, not from the tool name, so that listing projects, adding a project,
listing presets, launching, waiting, messaging, reading and lifecycle actions
are distinguishable at a glance. Those icons SHALL be monochrome and tinted with
the row's own text color, like the transcript's other non-file-type glyphs. An
unrecognized or absent action SHALL resolve to a neutral Workers glyph distinct
from the launch glyph. Header text, project and identity chips, disclosure,
status and invocation data SHALL remain unchanged.

#### Scenario: A turn calls several Workers actions
Test: unit — icon resolution table.
- **WHEN** one turn lists projects, adds a project, lists presets, launches a worker and waits for status
- **THEN** each row carries a different icon matching its action
- **AND** no row uses a saturated file-type icon

#### Scenario: An unrecognized action arrives
Test: unit — icon resolution table.
- **WHEN** a Workers call carries an action the transcript does not know, or no action at all
- **THEN** the row shows the neutral Workers glyph rather than a guessed one
- **AND** that glyph is not the one used for launching a worker
