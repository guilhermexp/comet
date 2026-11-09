# omp-startup Specification

## Purpose
Complete OMP startup protocol negotiation while preserving startup events even when extension output exceeds the live event-channel capacity.

## Requirements

### Requirement: Startup event bursts cannot block negotiation

The harness SHALL parse protocol negotiation responses without waiting for a consumer that is unavailable until startup completes. It SHALL retain startup event order within its transport memory limits and SHALL preserve cancellation and fatal error handling.

#### Scenario: Extensions emit a burst before the negotiation response

Test: integration — OMP RPC subprocess fixture.

- **WHEN** the child emits a startup burst exceeding the previous 256-event channel capacity but within the supported 1,024-event startup bound before its protocol negotiation response
- **THEN** startup completes and the subsequent request succeeds
- **AND** startup events remain available in original order

#### Scenario: Startup output exceeds the bounded event queue

Test: integration — OMP RPC subprocess fixture.

- **WHEN** startup events exceed the supported 1,024-event bound before a consumer is attached
- **THEN** the transport reports a clear fatal buffer-limit error and releases its child
- **AND** it does not masquerade as a protocol negotiation timeout

#### Scenario: An attached live consumer temporarily falls behind

Test: integration — OMP RPC subprocess fixture.

- **WHEN** the attached consumer temporarily pauses while live events fill the queue
- **THEN** existing live backpressure preserves events until the consumer drains them
- **AND** transport requests resume successfully after draining

### Requirement: Extension startup requests cannot deadlock negotiation

An interactive OMP Chat SHALL service extension input requests while startup hooks are running, including before negotiation or initial configuration completes. Startup SHALL retain cancellation, bounded buffering and fatal-error handling. Normal extension discovery and the existing skill overlay SHALL remain unchanged.

#### Scenario: Startup extension waits for user input
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** a startup extension emits a dialog and waits for its answer before accepting protocol negotiation or initial configuration
- **THEN** the Chat presents and answers the request through the existing input mechanism and completes startup
- **AND** startup events are delivered in order without duplication, and user response time is not misclassified as transport failure

#### Scenario: User stops during a startup dialog
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** the user stops the Chat while a startup extension request is pending
- **THEN** the pending request resolves once and the startup process and bridges terminate
- **AND** the adapter does not wait for the normal prompt deadline or leave a child process behind

### Requirement: Noninteractive discovery cannot wait for a dialog

OMP catalog discovery without a Chat input consumer SHALL use an explicit noninteractive extension policy. It SHALL retain normal user/project extension discovery and project skill scoping, and SHALL terminate with a result or bounded failure instead of waiting for an answer no user can provide.

#### Scenario: Catalog extension has an interactive startup hook
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** model or command discovery starts with an extension whose startup hook requests UI
- **THEN** discovery completes or fails within its bounded deadline without an unanswerable dialog
- **AND** extension commands remain discoverable, Chat launches remain interactive, and neither launch disables extension discovery
