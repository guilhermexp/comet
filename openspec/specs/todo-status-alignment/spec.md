# todo-status-alignment Specification

## Purpose

Give every To-dos row one shared status geometry so completed checks and the
current-item arrow sit centered in the same circular slot, aligned with the row
text on every row including the rounded last one.

## Requirements

### Requirement: Center every To-dos status mark

The Details To-dos widget SHALL use the same row geometry as the inline To-do
card and center completed/current glyphs inside the same fixed circular slot.

#### Scenario: Completed and current rows render together

Test: pure UI geometry contract plus headed GPUI smoke.

- **WHEN** the widget renders completed and current items
- **THEN** both glyphs are centered horizontally and vertically in 15 px circles
- **AND** the circles remain aligned with the text baseline across all rows
- **AND** the final rounded row does not shift the status slot

#### Scenario: Inline and Details rows render the same task list

Test: pure UI geometry contract plus headed GPUI smoke.

- **WHEN** both surfaces render completed and current items
- **THEN** rows use 36 px height, 12 px horizontal padding and 9 px gap
- **AND** both glyphs are centered in non-shrinking 15 px circles
- **AND** the text and status columns share the same rhythm
