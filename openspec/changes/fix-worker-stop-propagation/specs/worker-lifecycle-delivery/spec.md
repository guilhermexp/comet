# Spec Delta

## Purpose

Keep Worker lifecycle indicators current when a live hook delivery is missed and multiple local clients share event delivery.

## ADDED Requirements

### Requirement: Durable lifecycle events recover missed delivery

The system SHALL apply a newer persisted Worker lifecycle event by the next normal activity poll even if its live hook POST was lost. Recovery SHALL preserve runtime-generation and event-order checks and SHALL consume an unchanged persisted event at most once.

#### Scenario: A completed turn loses its live Stop
- Test: unit — shared activity engine and Comet activity derivation with isolated session files.

- **GIVEN** a Worker is working
- **WHEN** Stop is persisted but its live POST does not arrive
- **THEN** the next activity derivation reports idle without waiting five minutes

#### Scenario: Stale disk events cannot undo new activity
- Test: unit — shared activity engine with deterministic file timestamps.

- **WHEN** a persisted event predates a live event or belongs to an older runtime generation
- **THEN** it does not replace the current activity
- **AND** an unchanged Stop does not undo a later Codex output rearm

### Requirement: Registration age does not evict listeners

Writers of the shared hook-port registry SHALL preserve other registered listeners when another client registers, including when there are more than sixteen entries. Unregistration SHALL remove only the owner's port.

#### Scenario: Seventeenth client registers
- Test: unit — isolated Rust port registries with seventeen bound loopback listeners; Swift registry writer reviewed for equivalent behavior.

- **GIVEN** sixteen registered listeners
- **WHEN** a seventeenth client registers
- **THEN** all seventeen ports remain available for broadcast
- **AND** unregistering the new client preserves the preceding sixteen
