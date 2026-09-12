## Purpose

Allow users to save messages for execution after the current Chat turn, preserving durable ownership, ordering, cancellation and recovery independently of the frontend window.

## ADDED Requirements

### Requirement: After-turn delivery is distinct from steering

The system SHALL offer an explicit after-turn action without changing ordinary send/steering. It SHALL persist that intent in the Chat command ledger and SHALL NOT send it to the active agent before the current turn has settled.

#### Scenario: User chooses after-turn while a response is active
- **WHEN** the user queues a message during active generation
- **THEN** the current agent does not receive that message and the UI shows it as waiting for later execution
Test: integration — crates/engine/tests/run_controls_chat_id.rs; manual — native GPUI interaction

#### Scenario: User sends ordinary steering
- **WHEN** the user uses the existing ordinary send action during an active run
- **THEN** steering retains its current behavior instead of becoming queued delivery
Test: unit — crates/ui/src/composer.rs; integration — crates/engine/tests/run_controls_chat_id.rs

### Requirement: Queued messages execute in order at a safe boundary

The host SHALL execute eligible after-turn messages one at a time in document order, only after real turn settlement and outside compaction or AwaitingInput. Each message SHALL preserve its prompt, attachments and selected run configuration and SHALL use normal engine-owned resume behavior.

#### Scenario: Two messages wait behind an active turn
- **WHEN** messages B and C are queued while turn A is active
- **THEN** B waits for A, C waits for B, and neither is superseded or injected into its predecessor
Test: integration — crates/engine/tests/turn_quiesce.rs

#### Scenario: Input or maintenance blocks dispatch
- **WHEN** the preceding turn is awaiting input or context compaction is active
- **THEN** the after-turn message remains pending until the execution is actually eligible
Test: integration — crates/engine/tests/run_controls_chat_id.rs

#### Scenario: Message attachments are still in transit
- **WHEN** a queued message references attachment bytes not yet available on the host
- **THEN** it preserves its pending position and is not executed without those attachments
Test: integration — crates/engine/tests/queued_attachments.rs

### Requirement: Stop and failure prevent unintended continuation

Eligible execution controls SHALL remain reachable behind a waiting after-turn message. Stop SHALL invalidate the prior pending after-turn chain. Turn error, crash or uncertain recovery SHALL NOT auto-start or replay that chain; the message content and resolution SHALL remain recoverable for an intentional retry.

#### Scenario: Stop follows a message waiting for attachments
- **WHEN** Stop is submitted behind a queued message whose attachments have not arrived
- **THEN** Stop reaches the active run without waiting for those bytes and the prior queued message does not start afterward
Test: integration — crates/engine/tests/queued_attachments.rs

#### Scenario: A reply to agent input follows a waiting message
- **WHEN** a response to active agent input is submitted behind a deferred after-turn message
- **THEN** the input response is not blocked by the deferred message
Test: integration — crates/engine/tests/run_controls_chat_id.rs

#### Scenario: Host restarts after uncertain delivery
- **WHEN** the host restarts without proof that the preceding turn settled or a claimed message was delivered
- **THEN** it resolves the affected pending chain without automatic replay and retains recoverable content and a reason
Test: e2e — crates/engine/tests/restart_resume.rs

### Requirement: Pending state is durable and cancellable

Multiple queued messages SHALL remain independently visible across Chat navigation and frontend restart. A user MAY cancel only an entry permitted by the existing author-and-pending ownership rule; cancellation SHALL NOT claim to undo a message already dispatched. Transcript reconciliation SHALL preserve one user message per identity.

#### Scenario: User cancels one pending message
- **WHEN** its author cancels B while B and C are still pending
- **THEN** only B is cancelled and C remains queued, without deleting an already delivered transcript message
Test: unit — crates/doc/src/commands.rs; integration — crates/engine/tests/run_controls_chat_id.rs

#### Scenario: Another device cannot cancel a foreign entry
- **WHEN** a device attempts to cancel a pending message authored by a different device
- **THEN** the existing ownership rule rejects the cancellation
Test: unit — crates/doc/src/commands.rs

#### Scenario: Navigation and acknowledgement race
- **WHEN** the user navigates away and back while the host acknowledges or begins a queued message
- **THEN** the message remains associated with the correct Chat and its optimistic display reconciles without duplication
Test: unit — crates/ui/src/state.rs; integration — crates/engine/tests/run_controls_chat_id.rs

### Requirement: Unsupported hosts and expiry are explicit

A host unable to support after-turn delivery SHALL reject the intent without converting it to immediate steering or Run. The system SHALL retain recoverable input on unavailable/unsupported host and SHALL apply the existing command TTL and attachment failure rules without permanent pending entries.

#### Scenario: Older host receives the new intent
- **WHEN** the executing host does not recognize the after-turn command kind
- **THEN** the submission is not accepted as another command kind and the user can recover the text
Test: integration — crates/engine/tests/run_controls_chat_id.rs; unit — crates/doc/src/schema.rs

#### Scenario: Command expires or attachment delivery fails
- **WHEN** the existing TTL expires or required attachments fail delivery
- **THEN** the queued message receives a visible terminal resolution instead of waiting indefinitely
Test: unit — crates/doc/src/commands.rs; integration — crates/engine/tests/queued_attachments.rs
