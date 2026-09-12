## ADDED Requirements

### Requirement: Disclosure arrows follow event text
The transcript SHALL place event disclosure arrows immediately after their content, using text-width labels that can shrink and truncate in constrained columns. JavaScript eval rows SHALL omit a redundant language prefix when their icon already identifies JavaScript.

#### Scenario: Short and long event labels
Test: none — native GPUI visual acceptance.
- **WHEN** a tool, reasoning, task or turn summary has an expansion arrow
- **THEN** the arrow follows its text instead of occupying the far column edge
- **AND** long labels truncate without clipping the arrow

#### Scenario: JavaScript eval has a title
Test: unit — stream_copy projection.
- **WHEN** a JavaScript eval tool has a descriptive title
- **THEN** the row shows the title without a duplicate js prefix
- **AND** non-eval text and non-JavaScript language labels remain intact
