## ADDED Requirements

### Requirement: Interactive question history
The transcript SHALL present interactive question requests as question lifecycle content, never as a generic ask tool payload. Submitted answers SHALL survive document serialization and reload.

#### Scenario: Submitted answers
- **WHEN** a request is answered
- **THEN** the transcript displays a single Answer or Answers card with questions in original order and selected labels matched by question id
- **Test:** unit

#### Scenario: Pending questions
- **WHEN** a question call starts and its interactive panel becomes available
- **THEN** the transcript progresses from Asking question to the question and Waiting for response, with controls only above the composer
- **Test:** unit

#### Scenario: Legacy or skipped request
- **WHEN** stored answer data is absent or the request is skipped
- **THEN** the UI states that answers are unavailable or the question was skipped without inventing an answer
- **Test:** unit

#### Scenario: Native answer presentation
- **WHEN** answered questions are rendered
- **THEN** a rounded bordered card has a compact icon header and regular sans answer text below each brighter question, without duplicate ask headers
- **Test:** none — native screenshot verification

## MODIFIED Requirements

### Requirement: Generic tools use their names directly
Generic Unknown tool headers without a specialized presenter SHALL display their icon and tool name without Ran tool or Running tool prefixes. The name SHALL retain truncation and the primary neutral header tone. Question tools SHALL use the interactive question history presenter instead. Specific semantic tool labels, failure presentation, active indicators and expanded payloads SHALL remain available.

#### Scenario: Generic tool settles
Test: unit — native stream copy projection.
- **WHEN** a generic tool without a specialized presenter is pending or completes
- **THEN** its normal header text is only its tool name
- **AND** specialized eval, hub and command headers keep their semantic labels
- **AND** question tools render their question lifecycle and answers instead of an ask header

#### Scenario: Generic ask settles
Test: unit — native question projection.
- **WHEN** an ask tool is pending or completes
- **THEN** its specialized presenter displays Asking question, Waiting for response, or Answer/Answers as appropriate
- **AND** neither ask nor Ran tool is displayed as a generic header
