# Spec Delta

## ADDED Requirements

### Requirement: Session Details identifies the host device
The Session card SHALL display a read-only Device row in its Project section, using the selected context's device name from the shared registry. An explicit remote device SHALL never be displayed as the current local machine. Long names SHALL remain contained with their full value available on hover.

#### Scenario: Inspect local or remote Chat
Test: none — native gpui render has no render harness; visual acceptance and existing context projection.
- **WHEN** the user inspects a Chat or selected project in Details
- **THEN** the Device row shows the same registered name used by the sidebar for that host
- **AND** selecting another host updates the displayed device

#### Scenario: Local Workers context
Test: none — native gpui visual acceptance; existing context projection defines local Workers ownership.
- **WHEN** Details inspects a Workers context with no explicit target device
- **THEN** the Device row identifies the local engine device

#### Scenario: Missing device metadata
Test: none — visual fallback review; render has no harness.
- **WHEN** the selected device has no registered display name
- **THEN** the row shows Unknown device without substituting a different host
