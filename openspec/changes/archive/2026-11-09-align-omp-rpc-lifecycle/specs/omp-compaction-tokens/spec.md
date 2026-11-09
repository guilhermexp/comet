# Spec Delta

## ADDED Requirements

### Requirement: Structured manual compaction outcomes take precedence

For an explicit OMP compact operation, the adapter SHALL use a supported structured terminal outcome to distinguish committed work, skipped work, interruption and failure. Human-readable output SHALL NOT override that outcome. Existing context-before-completion ordering, deadlines and cancellation SHALL remain in force.

#### Scenario: Structured success does not require English output
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** an explicit compact operation receives an early acknowledgement and a successful `manual_compaction_end`, with changed or absent terminal prose
- **THEN** it reads available fresh context and publishes it before exactly one successful completion
- **AND** it does not complete at the acknowledgement or an unrelated intermediate `agent_end`

#### Scenario: Nothing needs compacting
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** the matching manual compaction ends with `skipped:true`
- **THEN** the operation ends as a no-op without a provider error or fabricated token reduction
- **AND** any available fresh context is published before completion

#### Scenario: Aborted, failed or unrelated operation
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** the active compaction is aborted, fails or expires
- **THEN** it reports a non-success outcome once, preserving any partial output
- **AND** an unsolicited manual compaction event does not finish another prompt or create a second completion

### Requirement: Legacy compaction has a bounded fallback

The adapter SHALL preserve existing local slash-command compaction for verified older runtimes lacking structured completion. The fallback SHALL remain cancelable and bounded, and SHALL NOT accept contradictory prose as success when a structured outcome is expected.

#### Scenario: Older runtime emits only command output
Test: integration — `crates/harness/tests/omp_rpc.rs`.

- **WHEN** a verified legacy runtime completes explicit compaction using only its supported command-output contract
- **THEN** success still publishes fresh context before completion and failure, timeout and cancellation remain distinguishable
- **AND** the adapter does not wait indefinitely for an event that runtime cannot emit
