## Purpose

Integrate missing upstream reliability and desktop capabilities while retaining the fork runtime and presentation contracts.

## ADDED Requirements

### Requirement: Safe submit and selection
Empty Enter during a live run SHALL not interrupt it. Code selection SHALL preserve source blank lines.

#### Scenario: Safe submit and selection regression
- **WHEN** an empty composer receives Enter during a live run
- **THEN** the run continues; explicit stop remains available
- **Test:** unit

### Requirement: Durable publication
Pending Chat updates SHALL survive host eviction and restart and be replayed idempotently.

#### Scenario: Durable publication regression
- **WHEN** the host stops before an update is acknowledged
- **THEN** the update remains pending and is delivered on restart
- **Test:** integration

### Requirement: Efficient discovery and login
Known live HTTP listeners SHALL not receive repeated discovery probes. Codex login SHALL use the same executable resolver as its harness.

#### Scenario: Efficient discovery and login regression
- **WHEN** a confirmed HTTP listener remains alive across discovery cycles
- **THEN** no redundant HTTP discovery request is sent
- **Test:** integration

### Requirement: Desktop controls
The UI SHALL provide action and Chat history search through Cmd+K and independent terminal and code font preferences, preserving fork typography and controls.

#### Scenario: Desktop controls regression
- **WHEN** a user changes the terminal font
- **THEN** code and diff font preferences remain unchanged
- **Test:** unit

### Requirement: Image artifacts and captures
Generated images SHALL display in the Chat. User-invoked desktop Appshots SHALL be delivered safely to the intended Chat and display on iOS.

#### Scenario: Image artifacts and captures regression
- **WHEN** the agent produces a supported image artifact
- **THEN** the Chat displays the image with safe bounded loading
- **Test:** integration

#### Scenario: Capture destination survives navigation
- **WHEN** a user navigates to another Chat or Workers while a capture is pending
- **THEN** the result is staged for the original destination and becomes visible for review
- **Test:** unit

#### Scenario: Observed application context stays out of visible messages
- **WHEN** an Appshot is sent with a window title and observed application text
- **THEN** the transcript displays its source and screenshot while hiding the observed context, including text resembling an attachment trailer
- **Test:** unit

### Requirement: Runtime and platform compatibility
OpenCode SHALL support the upstream 1.18 and 2.x wire formats. Menu and capture-platform integrations SHALL preserve local runtime and UI contracts.

#### Scenario: Runtime and platform compatibility regression
- **WHEN** OpenCode emits a supported 2.x event
- **THEN** the adapter projects it without losing session identity
- **Test:** integration
