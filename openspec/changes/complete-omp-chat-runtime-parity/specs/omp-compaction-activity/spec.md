## Purpose

Let users observe OMP context compaction in the native Chat using actual runtime activity, without invented progress or exposing internal context summaries.

## ADDED Requirements

### Requirement: Automatic and manual compaction are observable

The system SHALL indicate actual automatic and manual compaction activity in the Chat's existing activity surface. It SHALL distinguish completion, cancellation and failure using available runtime information, and SHALL NOT infer progress percentages or estimated completion times.

#### Scenario: Automatic maintenance begins and ends
- **WHEN** the runtime reports automatic compaction start followed by completion
- **THEN** the Chat shows compaction activity during the operation and removes it afterward
Test: integration — crates/harness/tests/omp_rpc.rs and crates/engine/tests/e2e.rs; manual — native GPUI visual confirmation

#### Scenario: Manual operation lacks automatic lifecycle events
- **WHEN** /compact runs and runtime state confirms compaction without automatic-start/end events
- **THEN** manual activity is visible while active and the local command result is delivered normally
Test: integration — crates/harness/tests/omp_rpc.rs; manual — native GPUI visual confirmation with installed OMP

### Requirement: Activity cannot outlive its execution

The system SHALL clear compaction activity on cancellation, failure, process exit or replacement by a newer run. Late activity from an older run SHALL NOT overwrite the current state. A maintenance retry SHALL NOT be represented as a completed user turn.

#### Scenario: Aborted or lost process
- **WHEN** active compaction is aborted or its process exits unexpectedly
- **THEN** the activity stops and the outcome is not presented as success
Test: integration — crates/harness/tests/omp_rpc.rs and crates/engine/tests/e2e.rs

#### Scenario: Old event arrives after a new run
- **WHEN** a previous run emits a late compaction event after a newer run is active
- **THEN** the new run's activity is unchanged
Test: unit — crates/engine/src/sessions.rs

#### Scenario: Maintenance schedules a retry
- **WHEN** compaction ends with a runtime continuation or retry indication
- **THEN** the Chat does not complete the turn or release a waiting after-turn message solely because maintenance ended
Test: integration — crates/harness/tests/omp_rpc.rs

### Requirement: Context indicators and privacy remain accurate

The system SHALL retain the last known context measurement until a new runtime measurement exists and SHALL keep occupancy distinct from compaction progress. Internal compaction summaries SHALL NOT be added to the Chat Transcript or exposed through new public event payloads. Older Session payloads without activity SHALL remain readable.

#### Scenario: No new context measurement is available
- **WHEN** compaction ends without a new context-usage snapshot
- **THEN** the indicator retains its last known measurement rather than guessing from the old token count or elapsed time
Test: unit — crates/engine/src/sessions.rs and crates/ui/src/composer.rs

#### Scenario: Activity is projected to older consumers
- **WHEN** a consumer reads an older Session payload or observes a Chat while the host compacts
- **THEN** absence of optional activity remains compatible and internal summary/context is not exposed through transcript or new raw-event traffic
Test: unit — crates/proto/src/entities.rs; integration — crates/engine/tests/e2e.rs

#### Scenario: Idle recap encounters compaction
- **WHEN** the idle-recap policy evaluates a Chat with active compaction
- **THEN** it does not generate an idle recap until the activity ends and normal idle conditions are met
Test: unit — crates/ui/src/details_sidebar/idle_recap.rs
