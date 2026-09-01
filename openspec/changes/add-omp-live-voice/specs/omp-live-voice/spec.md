## ADDED Requirements

### Requirement: Local OMP Live availability

The system SHALL offer Live Voice only when the selected Chat is hosted on the current device, uses OMP, is not archived, has no active run, no other Live call exists, and the installed OMP advertises `liveVoice`.

#### Scenario: Unsupported OMP is rejected
- **Test:** engine integration

- **WHEN** capability probing returns no `liveVoice`
- **THEN** start SHALL fail with an actionable OMP update reason
- **AND** start SHALL NOT mutate the Chat

### Requirement: Media remains inside OMP

The system SHALL transport only control, phase, level, transcript, delegation, and terminal frames between Comet and the Live child.

#### Scenario: Live conversation remains transient
- **Test:** harness integration + engine integration

- **WHEN** user and assistant exchange realtime speech without a delegation
- **THEN** no Chat message, command, CRDT field, DeviceRoom frame, upload, or log SHALL contain audio or casual transcript content

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
