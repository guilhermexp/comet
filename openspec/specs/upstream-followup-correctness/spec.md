# Upstream followup correctness Specification

## Purpose

Preserve the seven approved upstream correctness improvements across the native fork without replacing its established interaction contracts.

## Requirements

### Requirement: Browser geometry

The browser SHALL preserve fractional native geometry and reserve divider overlap for shell input.

#### Scenario: Browser geometry regression

- **WHEN** a browser pane is resized at fractional widths
- **THEN** its native frame matches the layout and divider clicks reach the shell
- **Test:** none — native fixture and BCU

### Requirement: Focus ownership

The UI SHALL restore composer focus once after explicit completion or dismissal and preserve click-away focus ownership.

#### Scenario: Focus ownership regression

- **WHEN** a picker is completed, escaped, or dismissed by clicking another surface
- **THEN** completion and Escape restore the composer while click-away preserves the target focus
- **Test:** unit; native BCU

### Requirement: Restricted Chat titles

Chat titles SHALL use an isolated, bounded, tool-free run with validated device-local harness/model preferences and a safe fallback. If no usable generated title is obtained, the Chat SHALL remain eligible for a later normal title-generation trigger instead of persisting prompt words as a definitive title. A failed generation SHALL NOT rename its worktree branch.

#### Scenario: Restricted Chat titles regression

- **WHEN** a title run requests a tool, times out, or has an unsupported configuration
- **THEN** no tool runs and title generation falls back without modifying project files
- **Test:** unit / integration

#### Scenario: Failed generation does not permanently name the Chat
Test: unit — isolated engine/workspace and controlled title harness.

- **WHEN** an untitled Chat receives no usable generated title after the existing bounded attempts
- **THEN** no definitive title is written and no automatic worktree rename occurs
- **AND** the interface retains its existing untitled placeholder

#### Scenario: A later normal trigger can generate a title after recovery
Test: unit — failure followed by success through the existing generator and workspace.

- **WHEN** generation previously failed for an untitled Chat and a subsequent normal generation trigger obtains a valid title
- **THEN** that generated title is persisted
- **AND** the earlier failure does not cause the later attempt to be skipped as already named
- **AND** this change does not add timers, a new retry ladder or provider switching

### Requirement: Completion notifications

The application SHALL notify completion only for a newly completed turn, preserving compatibility with older Session records.

#### Scenario: Completion notifications regression

- **WHEN** a run completes, is interrupted, or hands off to a queued send or steer
- **THEN** only actual new completion produces a completion notification and duplicate updates do not repeat it
- **Test:** unit / integration

### Requirement: Syntax coverage

Syntax highlighting SHALL recognize JavaScript-family, Kotlin and Dockerfile roles while preserving cached configurations and source text.

#### Scenario: Syntax coverage regression

- **WHEN** supported code including embedded Dockerfile commands is highlighted
- **THEN** semantic roles are produced without changing text or invoking unapproved injections
- **Test:** unit

### Requirement: Reasoning boundaries

Codex reasoning SHALL preserve paragraph boundaries per item and thread independently of chunking.

#### Scenario: Reasoning boundaries regression

- **WHEN** reasoning changes part or item or arrives from a child thread
- **THEN** paragraph separation is preserved without inserting breaks inside a continuing part
- **Test:** unit

### Requirement: Streaming navigation

The transcript SHALL preserve user scroll and selection intent while streaming geometry changes.

#### Scenario: Streaming navigation regression

- **WHEN** a user scrolls or begins selection during automatic following or own-turn alignment
- **THEN** automatic movement stops or resumes only according to user intent without crossing the prompt during remeasurement
- **Test:** unit; native BCU

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

### Requirement: Automatic titles preserve explicit and legacy names

Automatic title generation SHALL apply a generated result only while the Chat still exists and remains untitled at the moment of mutation. Existing titles, including legacy names whose origin is unknown, SHALL be preserved without guessing whether they were manual or fallback. No migration or bulk rename SHALL reinterpret existing title text.

#### Scenario: A Chat already has a title
Test: unit — actual title generator and isolated workspace.

- **WHEN** generation is requested for a Chat with a nonempty existing title
- **THEN** no title model is invoked and the existing title is unchanged
- **AND** a legacy title matching the initial prompt words receives the same protection

#### Scenario: User names the Chat while generation is in flight
Test: unit — controlled pending harness and conditional workspace mutation.

- **WHEN** a manual rename is committed before the automatic result is applied
- **THEN** that manual title is retained
- **AND** the automatic result does not overwrite it

#### Scenario: Chat is removed before the generated result returns
Test: unit — controlled pending harness and isolated workspace removal.

- **WHEN** the Chat is removed while title generation is pending
- **THEN** the automatic result does not recreate or rename that Chat
