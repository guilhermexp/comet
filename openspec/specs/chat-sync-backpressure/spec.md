# chat-sync-backpressure Specification

## Purpose
Keep Chat synchronization ordered and bounded when the server applies per-device push backpressure, without losing local updates or amplifying rejected traffic.

## Requirements

### Requirement: Quota rejection pauses eager Chat pushes

The client SHALL retain pending local Chat updates after a quota rejection and SHALL NOT send another push before the announced local retry deadline.

#### Scenario: Local updates arrive during quota cooldown

Test: `quota_rejection_blocks_enqueue_nudges_until_retry_deadline` in `crates/sync/src/chat_client/tests.rs` (`unit`).

- **WHEN** a Chat push is rejected for per-device quota
- **AND** another local update is queued before the retry deadline
- **THEN** the update remains pending
- **AND** no push is sent before the deadline

### Requirement: Quota recovery drains in order

After the retry deadline, the client SHALL retry only the blocked head update and SHALL continue with the next pending update only after the head is acknowledged.

#### Scenario: Cooldown expires with multiple pending updates

Test: `quota_retry_sends_one_head_then_ack_drains_next` in `crates/sync/src/chat_client/tests.rs` (`unit`).

- **WHEN** the quota cooldown expires with multiple pending updates
- **THEN** exactly the blocked head update is retried
- **AND** acknowledgement of that head permits the next update to be sent
- **AND** pending order is preserved

### Requirement: Canonical sync tests compile their fixtures

The repository SHALL provide a canonical `zeron-sync` test invocation that compiles and runs the registry integration test with its required mock-server support.

#### Scenario: Sync crate gate runs from the repository root

Test: `cargo test -p zeron-sync --features mock-server` (`integration`).

- **WHEN** the canonical sync crate gate is run from the repository root
- **THEN** the registry integration test compiles
- **AND** its test cases execute instead of failing on an unavailable fixture module
