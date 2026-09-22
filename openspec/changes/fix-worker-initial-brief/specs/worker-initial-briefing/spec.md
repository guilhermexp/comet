## Purpose

Ensure Workers using every configured OMP, Claude, Pi or Codex preset receive their assigned task during launch, without a separate task-delivery action.

## ADDED Requirements

### Requirement: Every configured runtime receives its task through launch
The system SHALL deliver a supplied initial task to OMP, Claude, Pi and Codex Workers through their supported native startup input, exactly once, without terminal prompt recognition or a second task submission.

#### Scenario: OMP initial task
- **WHEN** an OMP preset is launched with initial_text
- **THEN** native startup receives the task and no later task send is needed
- Test: integration — OMP native launch result

#### Scenario: Claude initial task
- **WHEN** a Claude preset is launched with initial_text
- **THEN** its interactive CLI receives the task as a native initial prompt without later task typing
- Test: integration — Claude native launch result

#### Scenario: Pi initial task
- **WHEN** a Pi preset is launched with initial_text
- **THEN** native startup receives the task without waiting for viewport readiness
- Test: integration — Pi native launch result

#### Scenario: Codex initial task
- **WHEN** a Codex preset is launched with initial_text
- **THEN** its interactive CLI receives the task as a native initial prompt without later task typing
- Test: integration — Codex native launch result

#### Scenario: Literal task content
- **WHEN** initial_text contains Unicode, quotes, shell metacharacters, leading options or file-like text, and trailing newlines
- **THEN** task content survives the runtime boundary literally, without shell evaluation, unintended argument parsing or newline truncation
- Test: integration — literal native task content across input formats

#### Scenario: Nonfatal startup warning
- **WHEN** a supported runtime retains startup or optional MCP warning text
- **THEN** that viewport text does not prevent native task submission or cause duplicate delivery
- Test: integration — native task delivery despite startup text

### Requirement: Launch preserves authority and lifecycle
The system MUST preserve the configured runtime options, checkout, model and approval behavior. Initial work MUST belong to the correct parent task. Restarting or resuming MUST NOT resubmit the initial task.

#### Scenario: Preset configuration and parent ownership
- **WHEN** a configured preset supplies runtime options or shares an id with a project-scoped preset
- **THEN** the actual enabled project-scoped-then-global command is preserved and initial completion belongs to the correct parent episode
- Test: integration — authoritative preset resolution and immediate completion

#### Scenario: Restart without task replay
- **WHEN** an OMP, Claude, Pi or Codex Worker with an initial task is restarted or resumed
- **THEN** the native task is not submitted again and the stored resume command contains no initial task
- Test: integration — one-shot restart for each runtime

#### Scenario: Existing permission and authentication gates
- **WHEN** the selected CLI requires authentication or action approval
- **THEN** the launcher preserves that gate and does not add bypass flags or claim model execution merely from submission
- Test: integration — permission flags preserved and submission distinct from execution

### Requirement: Delivery failures and task privacy remain explicit
The system SHALL distinguish process creation, initial task submission and task execution. Native startup failure MUST retain an inspectable worker identity and must not falsely acknowledge a missing task. Task bodies MUST NOT be persisted in restart commands, shell history or diagnostic argv logs.

#### Scenario: Native startup fails
- **WHEN** preparation or spawn fails, or the task body is missing
- **THEN** delivery is not falsely acknowledged; a known pre-submit failure remains retryable and any created worker stays inspectable
- Test: integration — missing body and failed native spawn

#### Scenario: Receipt failure after submission
- **WHEN** submission succeeded but its ACK cannot be persisted
- **THEN** the live Host remains alive, task material is not made replayable and guidance does not prescribe blind resubmission
- Test: integration — post-submit ACK failure preserves result without replay

#### Scenario: Diagnostic privacy
- **WHEN** Codex or Claude receives native positional task text
- **THEN** task text is absent from persisted command metadata, shell history and wrapper diagnostic logs
- Test: integration — positional task is not persisted in wrapper traces or resume metadata

#### Scenario: Non-submitting modes and unsupported integrations
- **WHEN** Host uses PasteOnly or Raw, or an integration lacks native startup capability
- **THEN** its existing submission contract and interactive shell/menu protections remain unchanged
- Test: integration — explicit modes and guarded fallback unchanged
