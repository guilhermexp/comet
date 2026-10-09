# connection-recovery Specification

## Purpose
Keep registry, Chat and device-room connections responsive through system suspend/resume and shutdown while preserving durable updates and remote-host availability evidence.

## Requirements

### Requirement: System resume refreshes active transport connections

The client SHALL replace active registry, Chat and device-room transport connections on a system-resume signal without waiting for the old receive lease. Durable updates SHALL remain available for replay, and ordinary consumer catch-up or sibling-online hints SHALL NOT disconnect a healthy session.

#### Scenario: A live connection becomes stale during suspension

Test: unit — sync actor tests; unit — RPC device-room fake relay.

- **WHEN** the system resumes with an old transport still open and silent
- **THEN** a fresh connection is attempted promptly
- **AND** pending durable updates remain queued for ordered replay

### Requirement: Registry shutdown retains cancellation ownership

The registry client SHALL stop promptly during dial, handshake or backpressured send. Cancelling the shutdown wait SHALL abort its owned actor rather than detach it, and shutdown SHALL cancel owned HTTP fallback work.

#### Scenario: Shutdown interrupts transport setup or blocked output

Test: unit — registry fake-transport lifecycle tests.

- **WHEN** shutdown begins during a pending dial, missing handshake or blocked send
- **THEN** the actor and its owned fallback work terminate without waiting for transport deadlines

#### Scenario: The shutdown waiter is cancelled

Test: unit — registry actor-drop regression.

- **WHEN** the caller cancels its registry shutdown wait
- **THEN** the registry actor is aborted and its transport resources are released

#### Scenario: Connection construction is cancelled

Test: unit — registry fake-transport lifecycle tests.

- **WHEN** a caller cancels registry construction before the initial handshake completes
- **THEN** the owned actor is aborted and its transport is released

### Requirement: Local resume preserves remote offline evidence

The client SHALL preserve an unexpired remote-host offline cooldown across local system resume. Fresh remote presence SHALL remain able to clear that cooldown and allow a new dial.

#### Scenario: The local device wakes while the remote host stays offline

Test: unit — RPC wake watcher and offline cooldown state regression.

- **WHEN** a remote host has reported offline and the local system resumes
- **THEN** no repeated dial is scheduled before the cooldown expires
- **AND** fresh remote presence can enable a dial earlier

### Requirement: Durable remote send acceptance survives lost wakes
An accepted remote command SHALL remain durably pending until the host reports its outcome. Within the server retry window, mobile suspension and a lost host wake SHALL recover through durable server handoff without duplicating the command. Server retries MAY stop on permanent routing rejection or 1,440 attempts; command rows SHALL remain available for host sync or sender reconnect after retry exhaustion.

#### Scenario: Sender suspends after command acceptance
Test: integration — live client mock-edge delivery and workerd wake recovery.
- **WHEN** a mobile sender suspends after the server accepts a command and the host misses the immediate wake
- **THEN** durable retry or reconnect makes the same command available to the host without creating duplicate user input

#### Scenario: Server wake retry budget expires
Test: integration — workerd wake recovery.
- **WHEN** a host remains unavailable until the server wake receipt reaches its 1,440-attempt cap
- **THEN** the bounded wake receipt may be removed while the accepted command rows remain available for host sync or sender recovery
