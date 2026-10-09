# live-voice-shutdown Specification

## Purpose
Ensure independent execution shutdown waits run concurrently while preserving each Chat settlement budget and durable state handling.

## Requirements

### Requirement: Independent execution shutdown waits run concurrently

The engine SHALL interrupt independent active Chats concurrently while preserving each run's existing settlement timeout and final durable state handling.

#### Scenario: Multiple runs ignore interruption

Test: unit — engine sessions shutdown regression.

- **WHEN** two or more independent active runs do not settle after interruption
- **THEN** shutdown does not wait the sum of their individual settlement budgets
- **AND** each run receives interruption
