# todo-status-alignment Specification

## Purpose

Give every To-dos row one shared status geometry so completed checks and the
current-item arrow sit centered in the same circular slot, aligned with the row
text on every row including the rounded last one.

## Requirements

### Requirement: Center every To-dos status mark

The To-dos widget SHALL center completed and current glyphs inside the same
fixed circular status slot aligned with the row text.

#### Scenario: Completed and current rows render together

Test: pure UI geometry contract plus headed GPUI smoke.

- **WHEN** the widget renders completed and current items
- **THEN** both glyphs are centered horizontally and vertically in 15 px circles
- **AND** the circles remain aligned with the text baseline across all rows
- **AND** the final rounded row does not shift the status slot
