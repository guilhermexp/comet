## ADDED Requirements

### Requirement: Tool code blocks wrap within the transcript
Expanded invocation and output code blocks SHALL wrap at the available card width without horizontal scrolling. Their height SHALL follow wrapped text up to the vertical viewport limit, and adjacent expanded cards SHALL have an 8px separation. Source whitespace, syntax highlighting and copy content SHALL remain intact.

#### Scenario: Long command and output
Test: none — native GPUI visual verification.
- **WHEN** a command or output line exceeds the available width
- **THEN** it wraps inside its code block and cannot shift the output sideways
- **AND** long content remains vertically scrollable

#### Scenario: Consecutive expanded cards
Test: none — native GPUI visual verification.
- **WHEN** two tool code blocks are expanded consecutively
- **THEN** an 8px gap separates their borders
