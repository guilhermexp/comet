# native-opencode-runtime Specification

## Purpose

Run OpenCode through its native bounded HTTP and event interfaces while preserving durable steering, model discovery, and fork-specific Workers projections.

## Requirements

### Requirement: OpenCode uses a bounded native runtime

The OpenCode harness SHALL start and control a native HTTP/SSE runtime with bounded startup and request deadlines.

#### Scenario: Native runtime becomes ready

Test: harness integration test using the real protocol against a controlled local fixture.

- **WHEN** an OpenCode run starts and its event subscription becomes live within the startup budget
- **THEN** the first prompt is submitted exactly once
- **AND** streamed text, Reasoning, tools, usage, and completion become normalized agent events

#### Scenario: Runtime or request stalls

Test: harness integration test with silent startup and hanging HTTP fixtures.

- **WHEN** startup, discovery, steering, or a prompt request exceeds its deadline
- **THEN** the run fails with a bounded provider-specific error
- **AND** no task remains forever in Working state

### Requirement: OpenCode model discovery reflects connected providers

The model picker SHALL offer only OpenCode models belonging to providers connected in the active OpenCode runtime.

#### Scenario: Catalog contains disconnected providers

Test: harness/UI unit test with connected-provider and model fixtures.

- **WHEN** model discovery returns models for both connected and disconnected providers
- **THEN** only connected-provider models are selectable
- **AND** the current model remains identifiable when valid

### Requirement: Workers activity remains projected

The native OpenCode runtime SHALL preserve the fork's tagged subagent lifecycle, transcript, steering, and completion events used by Workers surfaces.

#### Scenario: OpenCode child agent runs through the event bus

Test: harness integration test with parent and child event streams.

- **WHEN** OpenCode creates, updates, steers, and completes a child agent
- **THEN** events remain attributed to the correct parent spawn
- **AND** the parent run is not completed by a child terminal event
