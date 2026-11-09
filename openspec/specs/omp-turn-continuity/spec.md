# omp-turn-continuity Specification

## Purpose
Keep one OMP chat conversation on a stable provider request prefix across user turns, so later turns reuse the prompt cache instead of rewriting the whole history.

## Requirements

### Requirement: Completed OMP turn keeps its runtime warm
After an OMP chat turn completes cleanly, the system SHALL keep that OMP process and its host tools alive. The next user message with the same runtime configuration SHALL start a new turn in the same process, without spawning a process, switching sessions or re-registering host tools.

#### Scenario: Second user message reuses the process
Test: integration — OMP RPC subprocess fixture.

- **WHEN** a chat completes one OMP turn and the user sends a second message with the same model, reasoning, options and cwd
- **THEN** the second turn runs in the same OMP process
- **AND** the fixture records exactly one launch, one `switch_session` at most (from the first turn) and one `set_host_tools`
- **AND** each turn ends with exactly one `Done`

#### Scenario: Message sent while a turn is running is still a steer
Test: integration — OMP RPC subprocess fixture.

- **WHEN** the user sends a message while an OMP turn is still running
- **THEN** it is delivered as a steer into that turn, as before
- **AND** it does not start a separate turn

### Requirement: Warm OMP runtime retires through existing lifecycle paths
A warm OMP runtime SHALL still end through the existing idle reaper, Stop, runtime configuration change, harness update and engine shutdown. Each path SHALL terminate the OMP process and its host-tool bridges, and the next message SHALL resume the conversation in a new process.

#### Scenario: Idle reaper ends a warm runtime
Test: integration — engine with controlled harness.

- **WHEN** a warm OMP runtime stays idle past the session idle limit
- **THEN** the OMP process and its bridges exit
- **AND** the next user message resumes the same conversation in a new process

#### Scenario: Configuration change replaces the runtime
Test: integration — engine with controlled harness.

- **WHEN** the user changes the model or reasoning level of a chat whose OMP runtime is warm
- **THEN** the warm process is retired and a new one starts with the new configuration on the same conversation

### Requirement: Resumed OMP runtime reproduces the cached prefix
When an OMP conversation continues in a new process, the first provider request SHALL keep the prefix the previous process sent: the same system prompt, no tool-availability notice for host tools the conversation already had, and no previously accepted thinking block dropped.

#### Scenario: Resume after the process ended keeps the cache
Test: live — real `omp` binary and Anthropic provider, opt-in.

- **WHEN** an OMP conversation with host tools and thinking enabled completes a turn, its process ends, and a new process continues it with the same configuration
- **THEN** the recorded system prompt digest is unchanged
- **AND** the session records no tool-roster notice and no `thinking_dropped` transformation
- **AND** the first request of the new turn reads from cache at least the full prompt of the previous request, minus 5,000 tokens

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
