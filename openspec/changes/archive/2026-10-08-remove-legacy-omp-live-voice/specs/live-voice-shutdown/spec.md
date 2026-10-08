# Spec Delta

## REMOVED Requirements

### Requirement: Voice stop has one bounded cleanup budget
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Cancelling stop retains task ownership
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

## MODIFIED Requirements

### Requirement: Independent execution shutdown waits run concurrently

The engine SHALL interrupt independent active Chats concurrently while preserving each run's existing settlement timeout and final durable state handling.

#### Scenario: Multiple runs ignore interruption

Test: unit — engine sessions shutdown regression.

- **WHEN** two or more independent active runs do not settle after interruption
- **THEN** shutdown does not wait the sum of their individual settlement budgets
- **AND** each run receives interruption
