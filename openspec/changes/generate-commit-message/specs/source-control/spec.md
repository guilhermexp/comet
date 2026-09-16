## ADDED Requirements

### Requirement: Agent-generated commit draft
Changes SHALL offer generation of an editable commit draft from staged changes on the checkout's owning device, in Orchestrator and Workers. Generation SHALL be bounded, isolated from coding sessions, and SHALL NOT mutate Git or create a Chat turn.

#### Scenario: Staged changes only
- **WHEN** generation is requested on an authorized checkout
- **THEN** only staged diff content is supplied to an enabled supported text harness and a valid subject/body is returned
- **AND** empty staged changes fail before invoking the harness
- Test: integration

#### Scenario: Invalid response
- **WHEN** the harness fails, uses tools, returns invalid output or times out
- **THEN** generation fails visibly and retains the existing message
- Test: unit

#### Scenario: Draft ownership
- **WHEN** the user edits the message or switches context during generation
- **THEN** the stale generated response does not replace the user's current draft
- Test: unit

#### Scenario: Wand presentation
- **WHEN** Changes is visible in either mode
- **THEN** a discreet wand in the message field offers generation with staged changes and shows progress while running
- **AND** the generated message remains editable before an explicit Commit
- Test: none — native GPUI visual verification
