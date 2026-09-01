## ADDED Requirements

### Requirement: OMP tool normalizer maps grep and glob to typed tool calls

The OMP harness normalizer SHALL map `grep` to `ToolCall::Search` and `glob` to `ToolCall::Glob` so that search patterns and paths are preserved as typed tool calls rather than falling into `ToolCall::Unknown`.

#### Scenario: grep tool call is normalized to ToolCall::Search
Test: unit — normalizer grep mapping.

- **WHEN** an OMP tool event for `grep` is received with `pattern` and optional `path`
- **THEN** it is normalized into `ToolCall::Search` with the corresponding `pattern` and `path`

#### Scenario: glob tool call is normalized to ToolCall::Glob
Test: unit — normalizer glob mapping.

- **WHEN** an OMP tool event for `glob` is received with a `path` field
- **THEN** it is normalized into `ToolCall::Glob` extracting the pattern from the `path` field
- **WHEN** an OMP tool event for `glob` arrives without a `path` field
- **THEN** the pattern falls back to the `pattern` field rather than normalizing to an empty pattern
