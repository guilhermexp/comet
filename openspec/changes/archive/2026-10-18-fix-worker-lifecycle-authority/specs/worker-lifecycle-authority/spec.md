# Spec Delta

## Purpose

Keep Worker lifecycle, provider identity and parent attention aligned with the owning runtime across every catalog integration and event recovery path.

## ADDED Requirements

### Requirement: Lifecycle evidence belongs to the Worker runtime

The system SHALL identify the source runtime of every newly installed managed reporter and reject explicit cross-runtime lifecycle evidence before mutating Worker state. A declared primary-agent role MUST NOT authorize another runtime. The same admission decision SHALL apply to live delivery, preflight, activity recovery and journal recovery. Existing unidentified legacy primary events retain the handling required by the existing lifecycle specification.

#### Scenario: Embedded provider inherits Worker environment
Test: unit — Worker activity ingress and journal recovery.

- **WHEN** a process using a different runtime reports lifecycle with a live Worker's id and generation
- **THEN** the Worker's working/blocked state and primary provider binding remain unchanged
- **AND** no completion or attention notification is recorded for that event

#### Scenario: Durable evidence cannot bypass rejection
Test: unit — activity seed and journal recovery.

- **WHEN** a durable event identifies a different runtime or an identifiable nested conversation
- **THEN** replay rejects it before changing activity or recording a parent-notifiable event
- **AND** valid primary evidence from the current runtime still recovers after a missed HTTP delivery

### Requirement: Managed reporters respect admission before durable publication

Every managed reporter SHALL preserve the event's source runtime, generation and provider identity required for admission in its durable lifecycle seed, without persisting unrelated prompt or tool input. An explicit listener rejection SHALL suppress both delivery and durable publication in synchronous and asynchronous modes. Listener unavailability SHALL NOT by itself disable offline recovery.

#### Scenario: Rejected asynchronous hook
Test: unit — executable runtime reporter conformance.

- **WHEN** a known listener rejects a managed reporter's preflight
- **THEN** no state-changing POST or durable seed is published

#### Scenario: Listener unavailable
Test: unit — executable runtime reporter conformance.

- **WHEN** listeners cannot be reached for a valid lifecycle emission
- **THEN** an atomic identity-preserving lifecycle seed remains available for recovery

### Requirement: Runtime lifecycle capabilities match supported signals

The system SHALL audit all catalog runtimes and use only supported provider events for working, attention, resumption and completion. An intermediate continuation SHALL NOT count as completion. Hookless runtimes SHALL remain distinguishable from runtimes with reliable attention signals.

#### Scenario: Provider interaction lifecycle
Test: unit — executable provider reporters/extensions and Worker activity transitions.

- **WHEN** a supported primary interaction starts, waits for input, receives a response and finishes its turn
- **THEN** the Worker reflects working, blocked, working and completed in that order
- **AND** unsupported provider events are not used to claim reliable attention

### Requirement: Separate attention episodes notify the parent

A Worker attention event SHALL reach the bound parent Chat even if its previous turn has closed. Re-polling the same unresolved interaction SHALL NOT duplicate delivery. A later distinct interaction in the same task episode SHALL be eligible for delivery after the preceding one has resolved.

#### Scenario: Parent is idle when the Worker asks
Test: integration — Worker parent notifications and existing engine message-queue delivery.

- **WHEN** a primary Worker asks for input after the bound parent's turn ends
- **THEN** durable attention delivery is eligible to start a parent turn
- **AND** a second resolved-then-reopened question is not suppressed by the first acknowledgement

### Requirement: Current Prime runtime launches and reports native lifecycle

The system SHALL integrate the supported Prime Rust runtime without injecting unsupported JavaScript-extension flags. It SHALL preserve the interactive terminal, bind lifecycle observations only to the owning Worker's native runtime, and resume an explicit conversation rather than opening a newest-session picker. A native observer MUST NOT create an unrelated agent or control a user's unrelated daemon.

#### Scenario: Prime Rust starts a Worker
Test: integration — native launch compatibility and isolated lifecycle bridge.

- **WHEN** the Worker launches Prime Rust 0.10
- **THEN** the effective launch does not contain rejected JavaScript-extension flags
- **AND** the interactive terminal remains available
- **AND** actual native primary start/end signals drive the shared lifecycle path

#### Scenario: Native child activity cannot complete the primary
Test: unit — native event ownership and generation admission.

- **WHEN** a native child or unrelated session completes
- **THEN** the primary Worker's activity and provider identity are unchanged

#### Scenario: Resume identifies the owned native conversation
Test: integration — isolated native launch and resume contract.

- **WHEN** an existing Prime Worker resumes
- **THEN** the selected conversation belongs to that Worker
- **AND** resume does not silently pick another Worker's latest conversation or leave execution awaiting a picker
- **AND** an explicitly selected saved file outside the default catalog remains reachable after the old private daemon is replaced

#### Scenario: Native scope cleanup is launch-generation fenced
Test: integration — isolated native Host scope completion, resume and teardown.

- **WHEN** the native runtime returns to the owning shell and later resumes in the same PTY
- **THEN** the preceding scope's private daemon is stopped
- **AND** the replacement daemon belongs to the new accepted launch generation
- **AND** a delayed completion from the preceding launch cannot shut down that replacement

#### Scenario: Killing an in-flight native runtime is not completion
Test: integration — isolated native Host teardown while a loopback model response is held.

- **WHEN** the Host terminates Prime while its primary turn is still working
- **THEN** the private daemon is cleaned up
- **AND** teardown does not publish a synthetic `Stop` or successful completion
- **AND** the Host retains its real exited state

#### Scenario: Native shutdown leaves an owned stale socket
Test: integration — isolated native Host scope cleanup, with native shutdown unit regression.

- **WHEN** the owned Prime daemon stops accepting connections but leaves its socket inode
- **THEN** cleanup removes that stale owned endpoint so the next scope can bind it
- **AND** a live or unverified endpoint is not unlinked merely because shutdown was requested
