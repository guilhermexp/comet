# Spec Delta

## ADDED Requirements

### Requirement: Worker rows show launch age beside usage
The Workers widget SHALL show compact time since launch beside each Worker's token total, or beside its command fallback when tokens are unavailable. The age SHALL use the original creation time and the sidebar's relative-time units, remain readable in a narrow card and update while the widget is visible.

#### Scenario: Heartbeats and completion preserve launch age
Test: unit — timestamp projection; none — native gpui layout acceptance.
- **WHEN** a Worker receives output, heartbeat or terminal status updates
- **THEN** its displayed launch age remains derived from its creation time
- **AND** it does not reset when the Worker settles

#### Scenario: Missing usage or timestamp
Test: unit — absent and future timestamp handling; none — native gpui layout acceptance.
- **WHEN** token usage is missing
- **THEN** the command fallback and available launch age remain visible
- **AND** absent timestamps do not produce an invented age
- **AND** future timestamps clamp to the sidebar's current-time label
