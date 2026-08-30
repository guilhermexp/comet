## Purpose

Protect the native UI and durable Chat Transcript from reentrant state access and malformed producer output while preserving useful, privacy-safe recovery diagnostics.

## ADDED Requirements

### Requirement: Native interactions do not reenter borrowed window state

Interactive native UI paths SHALL complete without a reentrant window-state borrow failure.

#### Scenario: Reproduced interaction path updates the UI

Test: headed GPUI regression smoke for the identified callback (`e2e`).

- **WHEN** the user performs the interaction that previously caused a reentrant borrow
- **THEN** the intended UI state transition completes
- **AND** no `RefCell already borrowed` error is logged

### Requirement: Incremental Chat Transcript imports distinguish transient shells

An incomplete contentless part map observed during incremental CRDT import SHALL remain recoverable without being reported as durable transcript corruption.

#### Scenario: A part container arrives before its scalar fields

Test: transient-shell regression in `crates/doc/src/schema.rs` (`unit`).

- **WHEN** a part map has no content and contains no keys beyond incomplete `id`/`kind` identity fields
- **THEN** defensive salvage keeps the Chat readable
- **AND** the event is diagnostic debug data rather than a warning
- **AND** a content-bearing or unknown malformed map remains warning-level

### Requirement: Salvage diagnostics disclose structure only

When strict Chat Transcript parsing fails, diagnostics SHALL identify the failing structural path and inferred part kind while excluding transcript content, tool payloads, credentials, and other user data.

#### Scenario: A malformed part is salvaged

Test: schema diagnostic regression in `crates/doc/src/schema.rs` (`unit`).

- **WHEN** strict parsing fails because a required nested field is absent
- **THEN** the diagnostic records the structural field path and part kind
- **AND** it records salvage and drop counts
- **AND** it does not contain values from content-bearing fields

### Requirement: Defensive salvage remains available

Malformed remote or legacy entries SHALL continue through the existing best-effort salvage path so one invalid part does not make an otherwise recoverable Chat unreadable.

#### Scenario: One invalid part accompanies valid content

Test: existing and extended `salvage` schema tests (`unit`).

- **WHEN** an entry contains valid renderable parts and one invalid part
- **THEN** valid parts remain available
- **AND** the invalid part is dropped with a structural diagnostic

### Requirement: Incomplete local IPC handshakes are non-actionable

A TCP peer that closes before completing the local IPC WebSocket upgrade SHALL
be recorded as debug-level connection noise, while other handshake failures
remain warning-level diagnostics.

#### Scenario: TCP probe disconnects before WebSocket upgrade

Test: handshake error classification in `crates/rpc/src/server.rs` (`unit`).

- **WHEN** Tungstenite reports `ProtocolError::HandshakeIncomplete`
- **THEN** the server classifies the event as an incomplete peer disconnect
- **AND** malformed or rejected complete handshakes are not classified as benign

### Requirement: Unsupported GitHub checkouts are non-actionable

A checkout without a resolvable GitHub repository identity SHALL be treated as
an expected unsupported-provider state, while operational GitHub failures
remain warning-level diagnostics.

#### Scenario: Local or non-GitHub checkout is watched

Test: change-request diagnostic classification in
`crates/engine/src/change_requests.rs` (`unit`).

- **WHEN** provider resolution returns `UnsupportedRepository`
- **THEN** the refresh remains recoverable and records debug-level context
- **AND** authentication, rate-limit, timeout, decode, CLI, and command failures remain actionable warnings

### Requirement: Sign-out revokes cached peer links without a scheduling race

The peer-link cache SHALL observe credential removal even when sign-out happens
before its background supervisor receives its first runtime poll.

#### Scenario: Credentials are removed immediately after cache construction

Test: device-room sign-out regression in `crates/rpc/tests/device_room.rs` (`integration`).

- **WHEN** a peer-link cache is constructed and its credentials are then removed
- **THEN** the revocation subscription already exists
- **AND** every authenticated cached link is closed
- **AND** subsequent peer dials fail while signed out
