## ADDED Requirements

### Requirement: Runtime survives ordered sends until an explicit Stop
The system SHALL preserve a live runtime and its background work across ordinary sends, steers and configuration changes. Commands SHALL execute in sent order; explicit Stop SHALL cancel earlier pending delivery and terminate the owned process tree. Completion SHALL settle only the current turn and SHALL NOT fire while previously accepted input remains undelivered.

#### Scenario: Steer crosses a completion boundary
Test: integration — simulated provider and engine command-order regressions.
- **WHEN** a steer is accepted while the provider emits an old or intermediate completion
- **THEN** the accepted input is delivered once and the active replacement turn remains working until its own completion

#### Scenario: Stop reaches detached descendant shells
Test: integration — runtime survival and process-tree teardown fixtures.
- **WHEN** the user stops a runtime that owns commands in separate process groups
- **THEN** the turn and owned descendants terminate and cancelled queued input does not replay
