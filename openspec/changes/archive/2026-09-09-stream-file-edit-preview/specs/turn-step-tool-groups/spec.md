## ADDED Requirements
### Requirement: File edits show a bounded live typing preview
Write/Edit cards SHALL display progressive generated content in a 72px collapsed viewport, with at most 15 recent lines, bottom aligned after three lines. Text SHALL wrap with no horizontal scrolling. Partial input refreshes SHALL be time gated at 100ms after initial semantic previews, preserving bounded decoding and final content delivery. Active cards SHALL omit syntax highlighting and show filename shimmer and a spinner. Completion SHALL replace activity with line statistics and expansion affordances, requesting syntax highlighting after 50ms. Header and collapsed body SHALL expand the final file up to 200px with vertical scrolling. Errors SHALL remain visible.

#### Scenario: Small incremental input chunks
Test: unit — harness partial input decoder.
- **WHEN** a file tool streams small chunks beyond the initial preview
- **THEN** its preview refreshes after 100ms without requiring 16KB of new input
- **AND** final flush preserves the last decoded content

#### Scenario: Compact generated tail
Test: unit — ui file_change projection; none — native GPUI layout and shimmer.
- **WHEN** Write or Edit generates more than three lines
- **THEN** at most 15 generated lines are bottom aligned in a 72px viewport
- **AND** completion restores the authoritative diff and permits expansion to 200px
