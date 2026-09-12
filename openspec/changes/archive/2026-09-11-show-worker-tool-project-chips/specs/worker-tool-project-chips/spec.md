## Purpose
Make the project targeted by a Workers tool recognizable by its registered name in the Chat Transcript.

## ADDED Requirements

### Requirement: Named project target
The Chat SHALL display a Workers tool action followed by an @name chip when its effective project_id matches the local registered catalog and the Chat belongs to the local device. Original invocation and result data MUST remain unchanged.

#### Scenario: Exact registered project
- **WHEN** a local Workers call targets a registered project ID
- **THEN** the header uses that project's name, including after a catalog rename
- **Test:** unit

#### Scenario: Unresolved or different target
- **WHEN** the ID is unknown, the Chat is remote or unknown, the call targets a session, or the tool is not Workers
- **THEN** the existing technical header remains without a fabricated project chip
- **Test:** unit

#### Scenario: Mention appearance
- **WHEN** the resolved header is rendered during streaming or in an expanded settled turn
- **THEN** the action precedes an @name chip using the composer's mono font, mention background and rounded corners; disclosure, status and raw payload remain available
- **Test:** none — native gpui visual check; no render harness
