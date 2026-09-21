## Purpose

Ensure an OMP Worker receives its assigned task during launch, without requiring the caller to repair prompt delivery after startup.

## ADDED Requirements

### Requirement: Initial OMP task is delivered by launch
The system SHALL deliver a supplied initial task to an OMP Worker through the runtime's native startup input, exactly once, without requiring a second caller action or terminal prompt recognition.

#### Scenario: Nonfatal MCP startup warning
- **WHEN** an OMP Worker starts with an initial task and emits an optional MCP connection warning before accepting work
- **THEN** it receives and executes the task once, and launch reports successful briefing delivery without a separate send action
- Test: integration — native startup execution despite optional MCP warning

#### Scenario: Literal task content
- **WHEN** the initial task contains Unicode, quotes, newlines, shell metacharacters or text resembling command-line options or file references
- **THEN** the Worker receives task text rather than executing it as shell commands, interpreting it as flags or loading unintended files
- Test: integration — literal task bytes at the runtime boundary

### Requirement: Launch preserves authority and lifecycle
The system MUST preserve the chosen preset's runtime options, checkout and approval behavior. Initial submission MUST remain associated with the correct parent task, and restarting the Worker MUST NOT replay its original initial task.

#### Scenario: Preset and task identity survive native startup
- **WHEN** a parent launches an OMP Worker using an existing configured preset and initial task
- **THEN** the Worker uses that preset and exact checkout, does not acquire additional approval bypass, and its completion belongs to that initial task
- Test: integration — preset authority, checkout and initial task episode

#### Scenario: Restart does not repeat the first task
- **WHEN** a Worker originally started with an initial task is restarted or resumed
- **THEN** its initial task is not automatically submitted again
- Test: integration — resume does not replay initial task

### Requirement: Failure and other runtime behavior remain explicit
The system SHALL distinguish a created process from delivered work. A failed delivery MUST NOT report successful briefing submission, and a live Worker MUST retain an inspectable identifier. Non-OMP runtime delivery MUST retain its existing shell, boot and menu protections.

#### Scenario: Startup delivery fails
- **WHEN** a process is created but native initial delivery fails
- **THEN** the result identifies the Worker and reports the actual delivery failure without claiming successful submission
- Test: integration — partial launch reports delivery failure and session identity

#### Scenario: Spawn fails before ACK
- **WHEN** native argv is prepared but spawn or transport fails
- **THEN** `.attached` is absent, `.pending` remains, and `briefing_submitted` is false
- Test: integration — failed spawn does not report briefing_submitted

#### Scenario: Unauthorized submit modes keep PTY contracts
- **WHEN** Host create uses PasteOnly or Raw with an OMP command
- **THEN** native startup is not used and those submit-mode contracts remain
- Test: integration — PasteOnly/Raw do not use native startup

#### Scenario: MCP native decision matches Host command resolution
- **WHEN** a preset id is duplicated or carries a stale cli_id
- **THEN** MCP uses the enabled project-scoped then global command Host will spawn
- Test: integration — MCP native decision matches host command resolution

#### Scenario: Interactive runtime is not ready
- **WHEN** a runtime using interactive submission shows a shell, startup screen or blocking menu
- **THEN** the initial brief is not typed into that surface
- Test: integration — existing controller shell, boot and menu protections
