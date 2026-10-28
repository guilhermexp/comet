# Fork Upstream Refresh Specification

## Purpose

Keep accepted upstream Chat, workspace and desktop behaviors available while preserving the fork's security, managed Usage and panel ownership contracts.

## Requirements

### Requirement: Projectless Chats do not index the user's home

The application SHALL assign new projectless Chats their own host-owned scratch folder when the host supports the capability and SHALL exclude home roots from workspace search.

#### Scenario: Projectless execution resolves on its owning device
- **Test:** integration
- **WHEN** a projectless Chat runs on a scratch-capable host
- **THEN** its cwd is its own scratch folder, not the user's home, and Files/search resolve the same host-owned target

### Requirement: Device worktree preferences preserve fork ownership

The application SHALL support device-local worktree location preferences while preserving common Chat/Workers ownership, setup and deletion protections.

#### Scenario: Custom location is selected without bypassing setup
- **Test:** integration
- **WHEN** a Chat creates a worktree with a device location preference
- **THEN** creation uses that location and still follows the shared worktree lifecycle and reports setup errors without abandoning ownership

#### Scenario: Ancestor location does not falsely imply a nested checkout
- **Test:** unit
- **WHEN** the chosen location contains the repository but the common hashed destination is outside that repository
- **THEN** creation is allowed, while a destination inside the repository is refused before any checkout or branch is created

#### Scenario: Changing the destination does not skip historical preparation
- **Test:** integration
- **WHEN** setup failed for an owned checkout and the device's location preference changes before retry
- **THEN** retry prepares that same registered checkout through the ownership journal rather than treating it as external

### Requirement: Accepted desktop features coexist with fork surfaces

The application SHALL expose PR side panes, adaptive Session information, file multiselect, overscroll and idle harness icons without removing Details/Workers/Usage, the right-side terminal or Enter/Steer.

#### Scenario: Session metadata follows effective Details visibility
- **Test:** unit
- **WHEN** Details is visible or compressed to zero width
- **THEN** Session information omits duplicate location metadata only while Details has effective visible width and retains its actions

#### Scenario: Active Chat input remains steering
- **Test:** unit
- **WHEN** Enter submits nonempty input during an active run
- **THEN** the durable steer path retains attachments and the upstream queue does not replace it

### Requirement: Browser profiles remain isolated across identities

Embedded browser state SHALL persist within the owning identity's profile and SHALL NOT share stored website data across identities.

#### Scenario: Profile switch does not reuse another identity's storage
- **Test:** unit
- **WHEN** the identity changes or a browser tab is restored
- **THEN** profile selection restores only that identity's tabs/store and closing a tab preserves its profile's persistent state

### Requirement: Managed Usage reset information stays on its owning surface

Managed Provider Usage SHALL remain device-local in Details and Accounts and SHALL expose available reset time information without restoring the removed upstream usage ring.

#### Scenario: Reset tooltip preserves managed provider rows
- **Test:** unit
- **WHEN** a managed provider reports quota windows and reset timestamps
- **THEN** its existing Usage surface presents reset information while preserving provider ordering and local usage lines

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
