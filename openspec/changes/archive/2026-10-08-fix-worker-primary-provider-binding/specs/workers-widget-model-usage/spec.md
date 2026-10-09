## ADDED Requirements

### Requirement: Nested OMP subagents cannot replace primary Worker telemetry

The system MUST derive an OMP Worker's model identity and token totals from its primary provider conversation. A nested subagent's provider conversation MUST NOT replace that binding or contribute its model and tokens to the primary Worker's row.

#### Scenario: A nested subagent uses a different model
Test: unit + integration — lifecycle extension isolation and Worker hook provider binding.

- **WHEN** the primary OMP Worker uses Opus and a nested subagent uses Gemini
- **THEN** the primary Worker's provider binding remains the primary conversation
- **AND** the row's model and totals remain derived from the primary transcript
- **AND** the nested subagent's completion does not replace that telemetry

#### Scenario: A previous nested binding is replaced by primary evidence
Test: integration — durable binding replacement and telemetry refresh.

- **WHEN** a Worker has a previously stored nested subagent provider binding and receives valid primary conversation evidence
- **THEN** the binding and telemetry are replaced with primary conversation evidence
- **AND** subsequent nested lifecycle events cannot overwrite the corrected binding
