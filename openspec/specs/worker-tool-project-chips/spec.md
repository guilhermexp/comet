# worker-tool-project-chips Specification

## Purpose
Make the project targeted by a Workers tool recognizable by its registered name in the Chat Transcript.

## Requirements

### Requirement: Named project target
The Chat SHALL display a Workers tool action followed by an @name chip when its effective project_id matches the local registered catalog and the Chat belongs to the local device. Original invocation and result data MUST remain unchanged.

#### Scenario: Exact registered project
- **WHEN** a local Workers call targets a registered project ID
- **THEN** the header uses that project's name, including after a catalog rename
- **Test:** unit

#### Scenario: Unresolved or different target
- **WHEN** the ID is unknown, the Chat is remote or unknown, the call targets a session, or the tool is not Workers
- **THEN** no project chip is fabricated; unresolved targets retain technical identification
- **Test:** unit

#### Scenario: Mention appearance
- **WHEN** the resolved header is rendered during streaming or in an expanded settled turn
- **THEN** the action precedes an @name chip using the composer's mono font, mention background and rounded corners; disclosure, status and raw payload remain available
- **Test:** none — native gpui visual check; no render harness

### Requirement: Preset and Worker identity labels
Local Workers tool headers SHALL show the configured preset name and runtime icon for launch calls with a known preset_id, beside any resolved project chip. Session-targeted calls SHALL show the exact Worker's title and runtime icon instead of its opaque session_id. The existing runtime icon mapping SHALL be reused. Catalog names are presentation only and MUST NOT change execution inputs or results.

#### Scenario: Preset launch
- **WHEN** launch_worker references a known local preset and project
- **THEN** the header shows action, @project, and preset icon/name
- **Test:** unit

#### Scenario: Worker follow-up
- **WHEN** a local call references a known session_id
- **THEN** its header shows action and that Worker's current title/icon; session identity takes precedence over preset or project fields
- **Test:** unit

#### Scenario: Missing or remote identity
- **WHEN** a target cannot be resolved exactly, a legacy launch lacks preset_id, or the Chat is remote/unknown
- **THEN** no identity is guessed and technical identification remains available
- **Test:** unit

#### Scenario: Native chip
- **WHEN** a named preset or Worker header renders
- **THEN** its runtime icon and single-line name appear in a compact chip and existing status/disclosure behavior remains
- **Test:** none — native gpui inspection, no render harness

### Requirement: Safe preset identifier retention
The Chat Transcript SHALL preserve the bounded preset_id alongside existing Workers identifiers while excluding briefing, commands and unrelated input. Sanitization MUST remain idempotent.

#### Scenario: Launch input round-trip
- **WHEN** a Workers launch input includes preset_id and a private briefing
- **THEN** the sanitized call retains preset_id and existing identifiers, strips the briefing and remains unchanged on repeated sanitization
- **Test:** unit
