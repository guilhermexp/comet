## MODIFIED Requirements
### Requirement: Working indicator sits above the composer
The main Chat working indicator SHALL occupy the existing reserved status strip directly above the composer, aligned with the input column. It SHALL remain outside transcript scrolling and SHALL NOT also appear in the transcript rows. The transcript SHALL retain the former in-flow indicator footprint as blank space so docking moves only the indicator, not the last content. This reservation SHALL NOT mount a second spinner. The existing elapsed time, active spinner, sending, queued and retry states SHALL remain intact. Subagent panes SHALL retain their independent transcript indicator.

#### Scenario: Live reply grows or scrolls
Test: none — native GPUI positioning and existing presenter reuse.
- **WHEN** a Chat is working and its transcript grows or scrolls
- **THEN** its indicator remains directly above the composer with a small gap
- **AND** there is only one main Chat working indicator
- **AND** the last content keeps its previous spacing above the input
