## ADDED Requirements

### Requirement: Dated Claude snapshots reuse curated model rows
The Claude catalog SHALL map a dated snapshot to its undated curated row only when that row exists. Unknown snapshot IDs SHALL remain selectable, and default-model identity SHALL match the retained catalog row.

#### Scenario: Known and unknown snapshots
Test: unit — harness Claude catalog normalization fixture.
- **WHEN** discovery returns a curated Claude model and its eight-digit dated snapshot
- **THEN** the catalog exposes one curated row and maps the default to it
- **AND** an unknown dated model is retained

### Requirement: Icon and rolling label rendering preserve interface choices
Desktop SHALL adopt the upstream icon and rolling label presentation while retaining the fork's configured interface typography, pin-chip font independence and Files/Details/Workers ownership.

#### Scenario: Labels animate with fork navigation intact
Test: unit — icon/rolling text/pin fixtures; none — native visual acceptance.
- **WHEN** project labels, tabs or composer pin chips render using a selected interface font
- **THEN** glyphs and animated labels remain legible and pin-chip geometry remains independent of that font
- **AND** the fork's single Files/Changes surface and Worker views remain reachable
