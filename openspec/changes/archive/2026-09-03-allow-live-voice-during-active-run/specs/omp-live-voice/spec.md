## MODIFIED Requirements

### Requirement: Local OMP Live availability

The system SHALL offer Live Voice when an existing selected Chat is hosted on the current device, uses OMP, is not archived, has no other Live call, and the installed OMP advertises the capability required for the Chat's current Session state. Basic Live support SHALL permit start while the Session is `Idle`; starting while the Session is `Working` or `AwaitingInput` SHALL additionally require operational-context support. An `Idle` backend retained for warm OMP session reuse SHALL NOT count as active work. The system SHALL also offer the action on an OMP new-Chat draft targeting the current device; start-time validation remains authoritative.

#### Scenario: Working Session accepts Live
- **Test:** engine e2e + manual packaged smoke

- **WHEN** an otherwise eligible local OMP Chat has a `Working` Session
- **AND** OMP advertises operational-context support
- **THEN** Live Voice SHALL start without interrupting, replacing, or duplicating the active run

#### Scenario: Awaiting-input Session accepts Live
- **Test:** engine e2e

- **WHEN** an otherwise eligible local OMP Chat has an `AwaitingInput` Session
- **AND** OMP advertises operational-context support
- **THEN** Live Voice SHALL start without resolving or modifying the structured input request

#### Scenario: Older OMP remains compatible while idle
- **Test:** harness integration + engine e2e

- **WHEN** OMP advertises basic Live support but not operational-context support
- **THEN** Live Voice SHALL remain available while the Session is `Idle`
- **AND** Live Voice SHALL be unavailable with actionable update guidance while the Session is `Working` or `AwaitingInput`

#### Scenario: Completed parked run permits restart
- **Test:** engine e2e

- **WHEN** a Live delegation completes and its OMP backend is retained in `Idle` for warm reuse
- **AND** the user ends that Live call
- **THEN** Live Voice SHALL be available to start again immediately
- **AND** the retained OMP session SHALL remain reusable

#### Scenario: Unsupported OMP is rejected
- **Test:** engine integration

- **WHEN** capability probing returns no basic Live support
- **THEN** start SHALL fail with actionable OMP update guidance
- **AND** start SHALL NOT mutate the Chat

### Requirement: Media remains inside OMP

The system SHALL transport only control, phase, level, transient voice transcript, delegation, operational-context, and terminal frames between Comet and the Live child. Operational context SHALL contain only bounded display-safe Session status, visible assistant text, visible action labels, input-wait state, and visible errors; it SHALL exclude audio, reasoning deltas, raw Run Journal data, and protected tool payloads or results.

#### Scenario: Live conversation remains transient
- **Test:** harness integration + engine integration

- **WHEN** user and assistant exchange realtime speech without a delegation
- **THEN** no Chat message, command, CRDT field, DeviceRoom frame, upload, or log SHALL contain audio or casual transcript content

#### Scenario: Active-run context is silent and display-safe
- **Test:** engine unit + harness integration + engine e2e

- **WHEN** Live observes an active Session
- **THEN** context updates SHALL NOT initiate speech, delegation, or a durable Chat entry
- **AND** protected stream content SHALL NOT enter the Live context frame
- **AND** context delivery SHALL NOT backpressure or alter the coding run

### Requirement: Delegations use the durable run path

The system SHALL convert one Live delegation into one idempotent durable user instruction. If the originating Chat has a live steerable run, the instruction SHALL use that run's existing steer mailbox; otherwise it SHALL execute through the existing host executor and `SessionsEngine` run pipeline. The system SHALL return the visible backend answer to the same Live call as speakable final context before completing the delegation.

#### Scenario: Confirmed instruction steers the active run once
- **Test:** engine e2e

- **WHEN** Live emits one confirmed delegation while the originating Session has a live steerable run
- **THEN** exactly one user entry SHALL be durable
- **AND** exactly one steer SHALL enter the existing run
- **AND** the Live call SHALL remain active

#### Scenario: Run settles during confirmation
- **Test:** engine e2e

- **WHEN** the observed run settles before a confirmed delegation can be steered
- **THEN** the instruction SHALL execute as exactly one ordinary durable turn
- **AND** the instruction SHALL NOT also be steered

#### Scenario: Delegated coding work completes with speakable context
- **Test:** engine e2e

- **WHEN** the backend streams visible answer text and completes without a separate terminal result string
- **THEN** exactly one user entry and the normal backend transcript SHALL be durable
- **AND** the accumulated visible backend answer SHALL be returned to the Live call as final context
- **AND** spoken paraphrases SHALL remain transient

#### Scenario: Delegated coding work is persisted once
- **Test:** engine integration

- **WHEN** the Live child emits one delegation
- **THEN** exactly one user entry and one execution path SHALL be durable
- **AND** spoken paraphrases SHALL remain transient

## ADDED Requirements

### Requirement: Live observes active Session progress without acting

While Live Voice is active, the system SHALL provide the Live frontend with the latest bounded operational context for its originating Chat. Receiving or updating that context SHALL remain silent; Live SHALL use it only when responding to the user's speech and SHALL NOT announce progress proactively.

#### Scenario: Silence produces no action
- **Test:** OMP Live integration + engine e2e + manual packaged smoke

- **WHEN** Live is started while the Session is active
- **AND** the user remains silent
- **THEN** Live SHALL emit no assistant speech or delegation
- **AND** the Chat Transcript and active run SHALL remain unchanged

#### Scenario: Status question is read-only
- **Test:** OMP Live integration + manual packaged smoke

- **WHEN** the user asks what the active Session is doing
- **THEN** Live SHALL answer from current operational context
- **AND** it SHALL NOT emit a coding delegation or mutate the Chat

#### Scenario: Observed run settles
- **Test:** engine e2e

- **WHEN** the observed run completes or fails while Live remains active
- **THEN** the operational context SHALL transition to `Idle` or the visible error state
- **AND** the Live call SHALL remain active

### Requirement: Active-run instructions require voice confirmation

While operational context reports active work, Live SHALL ask for explicit user confirmation before emitting a delegation that changes or adds coding work. A rejected or unconfirmed proposal SHALL remain transient and SHALL NOT reach the coding run.

#### Scenario: Proposed instruction is rejected
- **Test:** OMP Live manual integration

- **WHEN** the user proposes a change and declines Live's confirmation
- **THEN** Live SHALL NOT emit a delegation
- **AND** no durable user entry SHALL be created

#### Scenario: Proposed instruction is confirmed
- **Test:** OMP Live manual integration + engine e2e

- **WHEN** the user confirms that an instruction should be sent
- **THEN** Live SHALL emit one host delegation containing that instruction
- **AND** Comet SHALL route it through exactly one durable execution path
