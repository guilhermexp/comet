# Spec Delta

## Purpose

Observe background commands, servers and apps owned by an orchestrating Chat using execution identity and lifecycle evidence from its host.

## ADDED Requirements

### Requirement: Background execution identity belongs to its originating Chat
The system SHALL associate a reported background execution with its originating Chat and host, using stable task or process identity. Long-running shell commands and server/app processes launched by the agent SHALL be observable while they remain alive. Rendering SHALL not start a provider turn or execute a command.

#### Scenario: A background command is reported
Test: integration — provider event/snapshot normalization and host projection.
- **WHEN** the runtime starts an identifiable background command such as `npm run dev` or `cargo run`
- **THEN** its identity, command/label, start time and observed lifecycle are exposed to that Chat
- **AND** an unrelated Chat or device does not inherit it

### Requirement: Background lifecycle is independent of its launcher
The system SHALL keep tracking background work after the initiating call or ordinary turn finishes. An app/server's confirmed live process SHALL remain active after its launcher exits. Completion SHALL require task/process lifecycle evidence; unavailable telemetry SHALL never be represented as successful completion.

#### Scenario: Server or app outlives the launching tool
Test: integration — background process fixture crossing launcher/turn completion.
- **WHEN** a launcher finishes while its associated server or app is still running
- **THEN** the background execution remains visible as active
- **AND** the tool or Chat settling does not falsely mark it completed
- **AND** the active state ends after confirmed execution exit

#### Scenario: Execution source is unavailable
Test: unit and integration — unavailable source and compatibility handling.
- **WHEN** the host or provider cannot report current background execution state
- **THEN** the UI exposes unavailable or unknown state rather than inventing running or completed work
- **AND** ordinary Chat/Worker lifecycle continues
