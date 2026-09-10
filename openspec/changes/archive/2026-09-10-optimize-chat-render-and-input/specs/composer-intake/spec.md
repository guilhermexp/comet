## ADDED Requirements

### Requirement: Attachment rows wrap within the available column
Staged and sent attachments SHALL wrap within their available column without clipping later attachments or requiring horizontal scrolling. Staged attachments SHALL remain above the input pill.

#### Scenario: Narrow column with multiple attachments
Test: none — native render acceptance at narrow and wide window widths.
- **WHEN** attachments exceed one row in the composer or a sent user message
- **THEN** every attachment remains visible on subsequent rows
- **AND** the input pill keeps its independent height
