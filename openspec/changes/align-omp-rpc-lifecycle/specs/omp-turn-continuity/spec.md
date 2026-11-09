# Spec Delta

## ADDED Requirements

### Requirement: Prompt outcomes are correlated and authoritative

The OMP adapter SHALL distinguish prompt admission from completion, correlate native prompt outcomes with the admitted request, and report its completed, interrupted or failed outcome exactly once. A terminal event with omitted messages SHALL NOT erase streamed output or override a native failure. Late or foreign outcomes SHALL NOT complete another prompt.

#### Scenario: Asynchronous local extension command
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** an admitted extension command completes locally through a matching `prompt_result` without an `agent_end`
- **THEN** its streamed output is preserved and the operation completes exactly once without waiting for a model turn
- **AND** a synchronous local completion in the command response still completes without requiring a later result

#### Scenario: Error with compacted terminal messages
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** streamed assistant output is followed by an `agent_end` with empty messages and the matching prompt outcome reports an error
- **THEN** the adapter preserves that output and reports the native failure rather than a successful completion
- **AND** an aborted outcome maps to interruption, without fabricated timing or usage

#### Scenario: Late outcome belongs to a previous prompt
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** a completed prompt's duplicate result or another request's result arrives while the next prompt is active
- **THEN** the active prompt is unaffected and neither prompt gains a second completion

### Requirement: Yield and session settlement remain distinct

The system SHALL present a yielded prompt's outcome without assuming the Session is settled. Continuations SHALL remain observable in the same runtime. Settlement without another run SHALL clear pending activity without another completion. Automatic idle retirement SHALL NOT discard known pending background delivery; explicit Stop and shutdown SHALL still work.

#### Scenario: Background delivery starts another run
Test: integration — `crates/harness/tests/omp_rpc.rs` and `crates/engine/tests/omp_warm_runtime.rs`.

- **WHEN** a prompt yields with background work pending and that work later starts another run
- **THEN** the prompt's answer completes once at yield and the later run streams through the same process and bridges
- **AND** the later run has its own completion and automatic idle retirement does not destroy the pending delivery

#### Scenario: Background delivery settles without a run
Test: integration — `crates/harness/tests/omp_rpc.rs` and `crates/engine/tests/omp_warm_runtime.rs`.

- **WHEN** a yielded run has `isTerminal:false` and later receives `session_settled` without another `agent_start`
- **THEN** the Session becomes idle without a stuck operation, duplicate completion or invented assistant message
- **AND** the next user message starts a fresh prompt in the warm runtime

#### Scenario: Internal continuation has not yielded
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** a retry, compaction continuation or reminder ends an internal run with `yielded:false`
- **THEN** it does not complete the user prompt or detach its pending interactive and host-tool work prematurely

### Requirement: Legacy completion remains supported

The adapter SHALL retain terminal-event completion for runtimes predating prompt outcomes and settlement signals. It SHALL determine protocol support explicitly without assuming that every v2 transport implements newer lifecycle features, and SHALL NOT invent success when a supported completion signal is missing.

#### Scenario: Legacy terminal event and modern missing outcome
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** a verified legacy runtime omits the newer lifecycle signals
- **THEN** terminal `agent_end` frames preserve the existing exactly-once behavior and warm reuse
- **AND** a modern runtime missing its expected outcome follows the bounded failure or cancellation path rather than silently falling back to success
