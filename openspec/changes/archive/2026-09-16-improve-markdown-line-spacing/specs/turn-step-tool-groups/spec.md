## ADDED Requirements

### Requirement: Readable spacing around inline code
Markdown body prose SHALL retain its 14px font with 24px line spacing. Inline chip backgrounds SHALL be inset 2px above and below their line box, producing 20px single-line chips in body prose. Chip-only lines SHALL retain the same outer line spacing. Streaming and settled responses SHALL use the same geometry, retaining file links and text selection.

#### Scenario: Dense prose containing inline code
- **WHEN** adjacent wrapped lines contain inline code or file references
- **THEN** their backgrounds remain visibly separated and text is vertically centered without changing font size
- **Test:** none — native visual QA; no render harness per crates/ui/AGENTS.md

#### Scenario: Streaming and chip-only lines
- **WHEN** a response streams and then settles, including lines made entirely of inline code
- **THEN** the line spacing stays consistent and links and selection remain available
- **Test:** none — native visual QA; existing selection and streaming unit regressions supplement inspection
