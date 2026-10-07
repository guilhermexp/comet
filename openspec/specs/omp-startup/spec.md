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
