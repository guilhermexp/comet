# resilient-registry-sync Specification

## Purpose

Keep durable Chat and registry state convergent when rows arrive out of order, acknowledgements are malformed, or newer devices emit unknown harness identities.

## Requirements

### Requirement: Registry cursors advance only across applied server rows

The registry client SHALL advance its durable cursor only through a contiguous sequence of server rows that it has applied.

#### Scenario: Push acknowledgement arrives before broadcast rows

Test: sync unit test with an acknowledgement followed by a gapped broadcast.

- **WHEN** the server acknowledges a local push at a sequence beyond rows received by the client
- **THEN** the client does not advance its pull cursor from the acknowledgement alone
- **AND** it requests and applies the missing server rows before advancing

### Requirement: Registry recovery waits for server truth

The engine SHALL defer orphan cleanup until registry synchronization is established and SHALL retry pushes whose HTTP acknowledgement cannot be decoded.

#### Scenario: Startup begins from an incomplete registry view

Test: engine integration test with delayed registry synchronization.

- **WHEN** local state appears orphaned before the first authoritative registry synchronization
- **THEN** the engine retains that state until server truth arrives
- **AND** cleanup decisions use the synchronized registry view

#### Scenario: Push response has an unreadable acknowledgement

Test: sync integration test with a malformed first acknowledgement and a valid retry.

- **WHEN** a registry push succeeds at the transport layer but its acknowledgement cannot be decoded
- **THEN** the batch remains pending and is retried
- **AND** no cursor or local mutation is lost

### Requirement: Forward-compatible Chat rows remain visible

The registry SHALL retain Chat rows whose optional runtime configuration contains a harness identity unknown to the current binary.

#### Scenario: Newer peer writes an unknown harness

Test: doc unit test decoding a newer-peer Chat row fixture.

- **WHEN** a Chat row contains a harness identifier unknown to this version
- **THEN** the Chat remains present and navigable
- **AND** only the unsupported optional runtime configuration is omitted

### Requirement: Diff reconciliation is bounded and stable

Diff synchronization SHALL avoid scheduling a new capture solely because its own reconciliation updated derived Chat metadata.

#### Scenario: Reconciliation observes unchanged checkout state

Test: engine unit test that counts captures across a reconciliation cycle.

- **WHEN** a completed diff reconciliation writes only its derived result
- **THEN** no immediate redundant capture is scheduled
- **AND** a real filesystem or Git state change still schedules one
