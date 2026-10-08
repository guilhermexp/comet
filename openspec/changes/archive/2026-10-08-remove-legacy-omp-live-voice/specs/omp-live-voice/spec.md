# Spec Delta

## REMOVED Requirements

### Requirement: Local OMP Live availability
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Live can create the first Chat
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Media remains inside OMP
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Live shares the Chat OMP session
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Delegations use the durable run path
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Competing commands stop Live
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Device-local lifecycle
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: macOS microphone declaration
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Live observes active Session progress without acting
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

### Requirement: Active-run instructions require voice confirmation
**Reason**: OMP Live Voice is retired at the user's request.
**Migration**: Use Codex Voice from the sidebar with the standalone Codex helper.

## ADDED Requirements

### Requirement: Retired OMP voice is unavailable
The system SHALL expose no OMP Live Voice action, capability probe or lifecycle in the Chat input. Legacy local OMP voice RPC methods SHALL return an unknown-method error without creating or mutating a Chat. Codex Voice SHALL remain available through its existing controls.

#### Scenario: Obsolete voice RPC request
Test: unit — RPC method registry and engine dispatch retirement checks.
- **WHEN** a client submits a retired OMP voice method
- **THEN** the method is not registered and no voice or Chat action is dispatched

#### Scenario: Chat input after retirement
Test: none — native gpui visual acceptance has no render harness.
- **WHEN** the user opens an existing Chat or a new Chat draft
- **THEN** no OMP voice microphone or strip appears in the input
- **AND** the sidebar Codex Voice trigger and active Codex call return affordance remain available
