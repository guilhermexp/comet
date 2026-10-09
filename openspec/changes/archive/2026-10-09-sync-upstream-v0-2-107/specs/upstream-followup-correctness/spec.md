## ADDED Requirements

### Requirement: Runtime survives ordered sends until an explicit Stop
The system SHALL preserve live runtimes and background work across ordinary sends, steers and supported reconfiguration. Commands SHALL execute in sent order. Explicit Stop SHALL terminate owned processes and cancel earlier user input while preserving genuine Worker notices, even when ordinary input stays paused. Completion SHALL settle only the current turn after accepted input is delivered.

#### Scenario: Steer crosses a completion boundary
Test: integration — simulated provider and engine command-order regressions.
- **WHEN** a steer is accepted while the provider emits an old or intermediate completion
- **THEN** the accepted input is delivered once and the active replacement turn remains working until its own completion

#### Scenario: Stop reaches detached descendant shells
Test: integration — runtime survival and process-tree teardown fixtures.
- **WHEN** the user stops a runtime that owns commands in separate process groups
- **THEN** the turn and owned descendants terminate and cancelled queued input does not replay

#### Scenario: A genuine Worker notice crosses Stop
Test: integration — held and pending Worker message-queue regressions.
- **WHEN** a genuine Worker notice is held or pending before Stop while ordinary input is queued
- **THEN** the notice is delivered once after Stop and remains outside the ordinary queue gate
- **AND** ordinary input remains paused until explicitly resumed

#### Scenario: Cancelled input cannot reappear after Stop
Test: integration — runtime-survival cancellation regression, with unit coverage of the atomic held-row guard.
- **WHEN** an ordinary prompt returns from a deferred delivery after a later Stop and the user sends a new prompt
- **THEN** the cancelled prompt is not reinserted or routed into the next live runtime
- **AND** the new explicit prompt remains deliverable

#### Scenario: Process-bound MCP grants change
Test: integration — Claude, Codex and OMP runtime identity regressions.
- **WHEN** a follow-up changes the Workers or root-session MCP grants bound to the live process
- **THEN** the incompatible runtime is replaced instead of accepting input under the old grants

#### Scenario: An OMP follow-up contains an image
Test: integration — OMP live-steer fixture.
- **WHEN** an image-bearing follow-up is routed to an existing OMP runtime
- **THEN** the bounded image payload accompanies its prompt without silently dropping the attachment

### Requirement: Detached OpenCode POST failures settle the turn
The system SHALL bound the complete OpenCode POST operation by the existing 60-second harness deadline and SHALL propagate transport failures, including body-read failures, to the active turn. Legitimate empty success responses SHALL remain accepted.

#### Scenario: A prompt POST never receives response headers
Test: unit — OpenCode fake-server bounded-timeout regression.
- **WHEN** the prompt body is accepted but the server withholds response headers
- **THEN** the harness deadline terminates the detached request and the session emits a terminal failure

#### Scenario: A prompt POST response body is truncated
Test: unit — OpenCode fake-server truncated-body regression.
- **WHEN** the server sends a successful status with an incomplete response body while its event stream remains open
- **THEN** the body transport error becomes a terminal failure instead of a successful empty acknowledgement
