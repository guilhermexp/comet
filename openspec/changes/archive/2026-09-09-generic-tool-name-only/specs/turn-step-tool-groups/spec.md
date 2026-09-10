## ADDED Requirements

### Requirement: Generic tools use their names directly
Generic Unknown tool headers SHALL display their icon and tool name without Ran tool or Running tool prefixes. The name SHALL retain truncation and the primary neutral header tone. Specific semantic tool labels, failure presentation, active indicators and expanded payloads SHALL remain available.

#### Scenario: Generic ask settles
Test: unit — native stream copy projection.
- **WHEN** an ask tool is pending or completes
- **THEN** its normal header text is only ask
- **AND** specialized eval, hub and command headers keep their semantic labels
