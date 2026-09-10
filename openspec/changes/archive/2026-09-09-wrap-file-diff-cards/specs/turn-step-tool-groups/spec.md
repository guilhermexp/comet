## ADDED Requirements
### Requirement: File diff cards wrap within their viewport
Write and Edit cards SHALL wrap long text within the available code column without horizontal scrolling. Old/new line numbers and diff signs SHALL remain aligned with the first visual line, and the change background SHALL cover the full width and height of each logical line. Number gutters SHALL fit the digits present and omit unused sides; truncated tails without reliable numbers SHALL NOT reserve empty number columns. Wrapping SHALL preserve source text, whitespace, syntax and change counts. Small previews SHALL size naturally up to their existing height cap; large fetched previews SHALL retain virtualization with variable row heights. Resizing SHALL remeasure wrapped rows. Vertical scrolling, lazy loading and rounded corners SHALL remain intact.

#### Scenario: Inspect long file changes
Test: none — native GPUI wrapping and variable-height list; unit — existing preview, gutter and height contracts.
- **WHEN** a user reads or expands a file change containing long indented lines
- **THEN** the entire line is available through wrapping and vertical scrolling without lateral scrolling
- **AND** continuation text stays in the code column, with full-width addition/removal backgrounds and no repeated logical line numbers
- **AND** large previews remain virtualized after resizing
