## Purpose

Expose provider-observed model identity and token usage for an OMP CLI Worker
Session without weakening Worker lifecycle, terminal, or compatibility paths.

## ADDED Requirements

### Requirement: A Worker exposes provider-observed model token usage

The Workers widget SHALL show the total tokens attributed to each effective
model recorded by an OMP Worker Session, without estimating model or usage
from terminal text, launch configuration, or global provider configuration.

#### Scenario: One OMP model produced the Worker responses
Test: unit — OMP JSONL normalization and Worker row projection.

- **WHEN** an OMP Worker transcript contains assistant messages with provider,
  model, thinking level, and non-negative `usage.totalTokens`
- **THEN** the Worker row shows the effective model identity and its accumulated
  token total
- **AND** the Session total equals the sum of its per-model totals

#### Scenario: The Worker changes effective model
Test: unit — ordered OMP model and thinking transitions plus disclosure projection.

- **WHEN** an OMP Worker changes model or thinking level between assistant
  messages
- **THEN** each assistant message is attributed to the identity effective when
  that message was produced
- **AND** the expanded breakdown lists the current identity first and every
  earlier identity with its own accumulated total

### Requirement: Provider evidence is bound to the exact Worker Session

The system MUST bind provider conversation identity and transcript location to
the Worker Session addressed by the lifecycle hook URL before exposing model
usage for that Worker.

#### Scenario: Hook carries provider conversation metadata
Test: integration — Worker lifecycle hook ingress and durable provider binding.

- **WHEN** an OMP lifecycle event reports a provider conversation id and
  transcript path for a URL-addressed Worker Session
- **THEN** the provider metadata is persisted for that Worker without replacing
  or confusing the Worker Session identity
- **AND** only a canonical provider JSONL path beneath the trusted OMP Session
  root can contribute telemetry

### Requirement: Worker model usage degrades without disrupting the Worker

Worker model usage MUST remain optional and device-local. Missing, malformed,
unsupported, or untrusted telemetry MUST NOT block Worker lifecycle, bootstrap,
terminal access, or widget rendering.

#### Scenario: Telemetry is unavailable
Test: unit — parser tolerance, optional wire decode, and UI fallback.

- **WHEN** provider evidence is absent, malformed, unsupported, or rejected as
  untrusted
- **THEN** the Worker row retains its existing command subtitle without an
  invented model or token value
- **AND** all existing Worker lifecycle and terminal behavior continues
  unchanged

#### Scenario: An older or non-OMP Session has no telemetry fields
Test: integration — backward-compatible Host bootstrap decoding.

- **WHEN** a Worker bootstrap record omits model usage fields
- **THEN** the record decodes successfully with no model usage
- **AND** no telemetry is synchronized through the Chat, edge, or Managed
  Provider Usage state

### Requirement: Worker telemetry disclosure preserves row behavior

The Workers widget SHALL use a stable per-Worker disclosure control when model
usage exists, while the remainder of the Worker row continues to open the same
Worker terminal.

#### Scenario: User expands model usage
Test: unit plus native gpui acceptance — stable disclosure identity and physical interaction.

- **WHEN** the user activates the telemetry disclosure for a Worker with one or
  more model totals
- **THEN** the widget reveals every model identity and its token total
- **AND** expansion remains associated with the same Worker when row ordering
  changes
- **AND** activating the rest of the row still opens that Worker's terminal
