# omp-live-voice Specification

## Purpose

Realtime bidirectional voice interaction for local Oh My Pi (OMP) coding sessions in Comet, supporting transient voice discussion, contextual progress observation, and voice-confirmed steering during active runs.

## Requirements

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

### Requirement: Live can create the first Chat

The system SHALL allow Live Voice to be the first action in a new-Chat draft by materializing an ordinary Chat with the selected target, Checkout, and OMP configuration before starting Live.

#### Scenario: New Chat starts with voice
- **Test:** UI unit + UI integration

- **WHEN** the new-Chat canvas targets the current device and selects OMP
- **THEN** the microphone action SHALL be visible and enabled without a prior text prompt
- **AND** invoking it SHALL create a normal Chat, start Live, and select that Chat only after start succeeds
- **AND** a failed creation or Live start SHALL remove the untouched empty Chat and preserve the user's current surface
- **AND** later text runs SHALL resume the OMP session created by Live

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

### Requirement: Live shares the Chat OMP session

The system SHALL start Live Voice with the Chat's stored OMP session identity when one exists and SHALL persist the effective OMP session identity when Live creates the first session for a Chat.

#### Scenario: Existing OMP session is resumed
- **Test:** harness integration + engine integration

- **WHEN** Live starts for a Chat with a stored OMP session in the same Checkout
- **THEN** the Live child SHALL switch to that session before `live_start`
- **AND** subsequent text runs SHALL resume the same identity

#### Scenario: Live creates the first OMP session
- **Test:** harness integration + engine integration

- **WHEN** Live starts for an eligible Chat without a stored OMP session
- **THEN** OMP SHALL create a normal session
- **AND** Comet SHALL persist its non-empty identity before exposing Live as active

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

### Requirement: Competing commands stop Live

The system SHALL stop an active Live frontend before executing any durable command other than the exact command owned by its active delegation.

#### Scenario: Text command arrives while Live is active
- **Test:** engine integration

- **WHEN** the host executor receives a different command ID
- **THEN** Live SHALL release microphone and playback before that command executes

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

### Requirement: macOS microphone declaration

The packaged macOS application SHALL declare a microphone purpose string and SHALL be smoke-tested from a signed Finder-launched app.

#### Scenario: Packaged permission grant
- **Test:** manual packaged smoke

- **WHEN** a user starts Live from the signed app for the first time
- **THEN** macOS SHALL present an attributable microphone permission flow
- **AND** successful grant SHALL allow OMP capture

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
