## ADDED Requirements

### Requirement: Preserve the fork while integrating upstream v0.2.126
The integration SHALL incorporate the upstream commits through `d5a01522` by real merge while retaining fork grants, Workers ownership, UI contracts, release feed and version 1.0.10.

#### Scenario: Cloud stays dormant
- **WHEN** the engine starts without `ZERON_RUNNER_*` and the Cloud status is unavailable
- **THEN** no Cloud runner SHALL start and the UI SHALL NOT offer Cloud pages or checkout
- **AND** provider switching between turns SHALL still work

Test: unit

#### Scenario: IPC clients authenticate
- **WHEN** a fork IPC client (`comet-sessions`, `zeron mcp`, Workers, auth/sync CLI) connects to the engine
- **THEN** it SHALL present the bearer token from the data dir
- **AND** a connection without the token SHALL be refused

Test: integration

#### Scenario: MCP grants stay root-only
- **WHEN** any run is dispatched, including Muse and runs with native subagents disabled
- **THEN** `RunRequest.mcp` SHALL be empty and `comet-sessions`/`zeron` SHALL reach only the root orchestrator with IPC
- **AND** native subagent tools SHALL only be disallowed when the run has the fork `sessions` grant

Test: unit

#### Scenario: CLI updates keep fork deltas
- **WHEN** an agent CLI update is planned
- **THEN** Pi SHALL run `pi update --all`, OMP and Antigravity SHALL keep their plans and the update SHALL hold the update lease
- **AND** the compatibility feed SHALL never be fetched from `zeron.sh`, and a 404 SHALL mean no known-bad versions

Test: unit

#### Scenario: Closing an unanswered question
- **WHEN** a question is closed without ever having an answer
- **THEN** its answers SHALL be recorded as empty
- **AND** cancelling SHALL NOT erase an already recorded answer, and absence SHALL remain distinct from empty

Test: unit

#### Scenario: Terminal stays in the right panel
- **WHEN** a terminal is opened, locally, from another device or by a Worker
- **THEN** it SHALL render in the right panel or the Workers terminal, never in a footer dock
- **AND** a remote device terminal link SHALL only be allowed between the user's own non-Cloud devices

Test: unit
