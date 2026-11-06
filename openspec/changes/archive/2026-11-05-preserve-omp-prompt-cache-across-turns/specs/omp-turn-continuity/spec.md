# Spec Delta

## Purpose

Keep one OMP chat conversation on a stable provider request prefix across user turns, so later turns reuse the prompt cache instead of rewriting the whole history.

## ADDED Requirements

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
