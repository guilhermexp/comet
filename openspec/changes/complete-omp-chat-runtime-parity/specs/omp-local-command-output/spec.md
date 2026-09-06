## Purpose

Preserve readable results and correct completion of OMP local slash commands in the native Chat, without invoking a model to replace lost output.

## ADDED Requirements

### Requirement: Local command output remains readable

The system SHALL display local command output in emission order in the Chat Transcript without creating a model request to explain it. Output SHALL remain available through existing reload and transcript export behavior.

#### Scenario: Local command returns multiple output frames
- **WHEN** an available local command such as /context emits several text frames and reports that no agent was invoked
- **THEN** the user can read the text once in the original order and the command completes
Test: integration — crates/harness/tests/omp_rpc.rs; manual — installed OMP smoke

#### Scenario: Result survives reopening the Chat
- **WHEN** a completed local result is followed by closing and reopening the Chat or exporting its transcript
- **THEN** the same output remains readable without rerunning the command or fabricating model usage
Test: e2e — crates/engine/tests/e2e.rs; manual — native transcript/export smoke

### Requirement: Local completion does not lose preceding events

The system SHALL process output preceding a successful local response before publishing completion. It SHALL NOT require an agent-end event when no agent was invoked or deadlock because output arrived before the response.

#### Scenario: Output exceeds the event-channel capacity
- **WHEN** a local command produces more pending output frames than the bounded channel can hold before returning its response
- **THEN** output consumption and response handling both progress and the command terminates after the output is delivered
Test: integration — crates/harness/tests/omp_rpc.rs

#### Scenario: Normal agent prompt remains compatible
- **WHEN** a prompt invokes the agent or the response omits the optional agent-invoked flag
- **THEN** normal streaming and completion remain unchanged
Test: integration — crates/harness/tests/omp_rpc.rs

### Requirement: Failure and long operations remain truthful

The system SHALL preserve any partial output when a local command fails, distinguish failure from successful empty output, and stop waiting when the process or request terminates. Long local operations SHALL have bounded operation-aware deadlines and remain cancellable rather than inheriting a short acknowledgement deadline.

#### Scenario: Local failure or premature process exit
- **WHEN** a command emits partial output and then fails or its process exits before responding
- **THEN** the output remains visible with a failure and the Chat does not stay Working indefinitely
Test: integration — crates/harness/tests/omp_rpc.rs

#### Scenario: Successful command has no text
- **WHEN** a local command succeeds without text output
- **THEN** the execution completes without invented assistant content
Test: integration — crates/harness/tests/omp_rpc.rs

#### Scenario: Long local work is cancelled
- **WHEN** legitimate local work exceeds the ordinary ACK duration and the user then cancels it
- **THEN** it is not prematurely reported as an ACK timeout and cancellation terminates the wait
Test: integration — crates/harness/tests/omp_rpc.rs
