## ADDED Requirements

### Requirement: Wide tables bleed past the prose column
A Markdown table SHALL be allowed to extend symmetrically past the prose column,
up to a bounded maximum width, when its natural width exceeds that column. The
extension SHALL apply to the table block alone: paragraphs, lists, headings and
code blocks in the same message SHALL keep the prose column width. A table that
already fits SHALL NOT be stretched. The extension SHALL be limited by the
measured viewport so that no content is pushed outside the visible area, and the
existing horizontal scroller SHALL remain for tables wider than the bound.
Column floors, wrapping, chip decoration and copy behavior SHALL be unchanged.

#### Scenario: A table is wider than the prose column
Test: unit — bleed arithmetic against natural width, column and budget.
- **WHEN** a message contains a table whose natural width exceeds the prose column
- **THEN** the table extends equally on both sides, up to the bounded maximum
- **AND** the surrounding paragraphs keep the prose column width

#### Scenario: A narrow window or a narrow table
Test: unit — bleed arithmetic at small viewports and for a fitting table.
- **WHEN** the viewport leaves no room beyond the prose column, or the table already fits
- **THEN** the table takes no extension and stays aligned with the prose
- **AND** nothing is positioned outside the visible area

#### Scenario: A table wider than the bound
Test: unit — bleed arithmetic above the maximum.
- **WHEN** a table's natural width exceeds the bounded maximum
- **THEN** it extends to the bound and the remaining overflow stays in the horizontal scroller
