# omp-live-voice Specification

## Purpose
TBD - created by archiving change add-omp-live-voice. Update Purpose after archive.

## Requirements

### Requirement: Local OMP Live availability

The system SHALL offer Live Voice when an existing selected Chat is hosted on the current device, uses OMP, is not archived, has no active run, no other Live call exists, and the installed OMP advertises `liveVoice`. It SHALL also offer the action on an OMP new-Chat draft targeting the current device; start-time validation remains authoritative.

#### Scenario: Unsupported OMP is rejected
- **Test:** engine integration

- **WHEN** capability probing returns no `liveVoice`
- **THEN** start SHALL fail with an actionable OMP update reason
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

The system SHALL transport only control, phase, level, transcript, delegation, and terminal frames between Comet and the Live child.

#### Scenario: Live conversation remains transient
- **Test:** harness integration + engine integration

- **WHEN** user and assistant exchange realtime speech without a delegation
- **THEN** no Chat message, command, CRDT field, DeviceRoom frame, upload, or log SHALL contain audio or casual transcript content

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

The system SHALL convert one Live delegation into one idempotent `SessionCommandPayload::Run` and SHALL execute it through the existing host executor and `SessionsEngine` pipeline.

#### Scenario: Delegated coding work is persisted once
- **Test:** engine integration

- **WHEN** the Live child emits one delegation
- **THEN** exactly one user entry and the normal backend transcript SHALL be durable
- **AND** spoken paraphrases SHALL remain transient

### Requirement: Competing commands stop Live

The system SHALL stop an active Live frontend before executing any durable command other than the exact command owned by its active delegation.

#### Scenario: Text command arrives while Live is active
- **Test:** engine integration

- **WHEN** the host executor receives a different command ID
- **THEN** Live SHALL release microphone and playback before that command executes

### Requirement: Device-local lifecycle

The system SHALL keep Live operations local to the host engine and SHALL release the Live child on End, Escape, Chat switch, surface close, engine shutdown, transport failure, or app quit.

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
