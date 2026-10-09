# Spec Delta

## Purpose

Ensure host Live Voice teardown releases owned tasks within its existing shutdown budget, including blocked control output and cancellation of the caller.

## ADDED Requirements

### Requirement: Voice stop has one bounded cleanup budget

The engine SHALL finish a Live Voice stop within the existing two-second cleanup budget even when the control channel is full or the observer ignores cooperative cancellation. It SHALL abort unfinished owned work and project Idle status. Repeated stop SHALL remain idempotent. New call admission SHALL remain blocked while the prior call is stopping, including when its stop waiter is cancelled.

#### Scenario: Voice output or observer is blocked

Test: unit — engine Live Voice coordinator lifecycle tests.

- **WHEN** stop encounters a full control channel or an observer that ignores cancellation
- **THEN** it returns within the voice cleanup budget and releases the observer
- **AND** the projected status is Idle


#### Scenario: A new call is requested during teardown

Test: unit — engine Live Voice reservation lifecycle regression.

- **WHEN** a new call is requested while the old observer is still stopping
- **THEN** admission is rejected until cleanup finishes
- **AND** repeated stop does not release that reservation prematurely
- **AND** cancellation releases the old observer and allows a subsequent call with its correct state

### Requirement: Cancelling stop retains task ownership

Cancelling a stop waiter SHALL release its owned observer rather than detach it.

#### Scenario: The stop caller is cancelled

Test: unit — engine Live Voice coordinator cancellation regression.

- **WHEN** the stop waiter is cancelled before the observer exits
- **THEN** the observer is aborted and its resources are released

### Requirement: Independent execution shutdown waits run concurrently

After stopping host voice, the engine SHALL interrupt independent active Chats concurrently while preserving each run's existing settlement timeout and final durable state handling.

#### Scenario: Multiple runs ignore interruption

Test: unit — engine sessions shutdown regression.

- **WHEN** two or more independent active runs do not settle after interruption
- **THEN** shutdown does not wait the sum of their individual settlement budgets
- **AND** each run receives interruption
