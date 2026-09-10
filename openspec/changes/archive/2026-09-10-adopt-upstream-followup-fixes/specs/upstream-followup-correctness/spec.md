## Purpose

Preserve the seven approved upstream correctness improvements across the native fork without replacing its established interaction contracts.

## ADDED Requirements

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

Chat titles SHALL use an isolated, bounded, tool-free run with validated device-local harness/model preferences and a safe fallback.

#### Scenario: Restricted Chat titles regression

- **WHEN** a title run requests a tool, times out, or has an unsupported configuration
- **THEN** no tool runs and title generation falls back without modifying project files
- **Test:** unit / integration

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

