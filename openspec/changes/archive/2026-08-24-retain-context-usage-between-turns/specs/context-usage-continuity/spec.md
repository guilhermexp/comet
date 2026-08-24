## ADDED Requirements

### Requirement: Retain the last context measurement between turns

The engine SHALL preserve the last context usage snapshot for each chat while a
new turn waits for a newer runtime measurement.

#### Scenario: A new process starts for a measured chat

Test: engine unit regression over turn-start state transition.

- **WHEN** a chat has a context usage snapshot
- **AND** its next turn starts a fresh harness process
- **THEN** the existing snapshot remains visible
- **AND** a newer runtime snapshot replaces it when reported

#### Scenario: A chat has never reported usage

Test: existing composer neutral-state unit test.

- **WHEN** the selected chat has no context measurement
- **THEN** the indicator remains in its neutral first-turn state
