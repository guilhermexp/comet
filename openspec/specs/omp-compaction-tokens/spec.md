# omp-compaction-tokens Specification

## Purpose
Make the context token reduction from OMP local compaction available to the Chat completion marker.

## Requirements

### Requirement: Refresh usage before local command completion
The OMP adapter SHALL publish available post-command context usage before reporting a successful local slash command complete. The existing Chat compaction marker SHALL be able to use that updated count alongside its pre-command count.

#### Scenario: Compaction reduces context
- **WHEN** `/compact` completes locally and the runtime reports 63000 context tokens
- **THEN** the adapter publishes 63000 context tokens before exactly one successful completion, enabling the existing before/after marker
- **Test:** integration — `crates/harness/tests/omp_rpc.rs`

#### Scenario: Usage is unavailable
- **WHEN** a local command succeeds but its state has no usage or its state request fails
- **THEN** completion still succeeds exactly once without fabricated token counts or duplicate command output
- **Test:** integration — `crates/harness/tests/omp_rpc.rs`

### Requirement: Await actual local compaction completion
For an explicit OMP compact command, Comet SHALL keep the operation active after an early prompt acknowledgement and await its terminal result. It SHALL publish the post-compaction context snapshot before completing the operation, without requiring a subsequent model turn. Cancellation, failure and expiry SHALL not produce a successful compaction marker.

#### Scenario: Background compaction acknowledges early
- **WHEN** OMP acknowledges the prompt before compaction finishes
- **THEN** the old context snapshot is not treated as the completed result and the final context is published before completion
- **Test:** integration — crates/harness/tests/omp_rpc.rs

#### Scenario: Background compaction fails or never completes
- **WHEN** compaction reports failure, is cancelled or exceeds its deadline
- **THEN** the run ends with the corresponding non-success status rather than an invented successful result
- **Test:** integration — crates/harness/tests/omp_rpc.rs

### Requirement: Display all reported compaction outcomes
The Chat marker SHALL show the pre-command and post-command context counts when available, including increases and equality, without CLI decoration or shortcuts. The gauge SHALL consume the fresh snapshot at completion, and the marker SHALL capture it without a delayed read of a subsequent turn's state.

#### Scenario: Compaction increases context
- **WHEN** the displayed context was 16000 tokens and compaction finishes with 59000 tokens
- **THEN** the marker shows `Context compacted · 16k → 59k` and the gauge reflects 59000 tokens before another turn starts
- **Test:** unit for marker — crates/ui/src/shell.rs; integration for usage-before-completion — crates/harness/tests/omp_rpc.rs

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
