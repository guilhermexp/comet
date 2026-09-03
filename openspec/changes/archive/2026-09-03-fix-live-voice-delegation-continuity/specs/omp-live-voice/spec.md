## MODIFIED Requirements

### Requirement: Local OMP Live availability

The system SHALL offer Live Voice when an existing selected Chat is hosted on the current device, uses OMP, is not archived, has no backend in `Working` or `AwaitingInput`, has no other Live call, and the installed OMP advertises `liveVoice`. An `Idle` backend retained for warm OMP session reuse SHALL NOT count as an active run. The system SHALL also offer the action on an OMP new-Chat draft targeting the current device; start-time validation remains authoritative.

#### Scenario: Completed parked run permits restart
- **Test:** engine e2e

- **WHEN** a Live delegation completes and its OMP backend is retained in `Idle` for warm reuse
- **AND** the user ends that Live call
- **THEN** Live Voice SHALL be available to start again immediately
- **AND** the retained OMP session SHALL remain reusable

#### Scenario: Unsupported OMP is rejected
- **Test:** engine integration

- **WHEN** capability probing returns no `liveVoice`
- **THEN** start SHALL fail with an actionable OMP update reason
- **AND** start SHALL NOT mutate the Chat

### Requirement: Delegations use the durable run path

The system SHALL convert one Live delegation into one idempotent `SessionCommandPayload::Run`, execute it through the existing host executor and `SessionsEngine` pipeline, and return the visible backend answer to the same Live call as speakable final context before completing the delegation.

#### Scenario: Delegated coding work completes with speakable context
- **Test:** engine e2e

- **WHEN** the Live child emits one delegation
- **AND** the backend streams visible answer text and completes without a separate terminal result string
- **THEN** exactly one user entry and the normal backend transcript SHALL be durable
- **AND** the accumulated visible backend answer SHALL be returned to the Live call as final context
- **AND** spoken paraphrases SHALL remain transient

#### Scenario: Delegated coding work is persisted once
- **Test:** engine integration

- **WHEN** the Live child emits one delegation
- **THEN** exactly one user entry and the normal backend transcript SHALL be durable
- **AND** spoken paraphrases SHALL remain transient

### Requirement: Device-local lifecycle

The system SHALL keep Live operations local to the host engine. Chat selection changes, clearing the selected Chat surface, window focus loss, and minimization SHALL NOT release the Live child. The system SHALL release the Live child on explicit End, Escape while controlling the active Live Chat, a competing durable command, engine shutdown, transport failure, or app quit.

#### Scenario: Navigation during delegated work
- **Test:** UI unit + engine e2e

- **WHEN** the user selects another Chat or leaves the Chat surface while a Live delegation is running
- **THEN** the Live call SHALL remain active on its originating Chat
- **AND** delegated progress and final speakable context SHALL continue through the host engine
- **AND** returning to the originating Chat SHALL show the current Live state

#### Scenario: Repeated stop
- **Test:** engine unit

- **WHEN** stop is called after Live has already ended
- **THEN** it SHALL succeed without spawning work or emitting duplicate terminal state
