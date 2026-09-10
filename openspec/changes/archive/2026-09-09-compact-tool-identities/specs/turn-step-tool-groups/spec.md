## ADDED Requirements
### Requirement: Tool headers identify the invocation without redundant execution copy
Skill headers SHALL show the invoked skill identifier beside Skill when supplied in the invocation. Hub headers SHALL show the operation and target detail without Ran hub or Running hub. Missing identifiers SHALL retain a truthful tool-name fallback. Error presentation and expandable payloads SHALL remain available. Rust and edge render sanitizers SHALL retain only the skill identifier keys (`skill`, `path`, `name`) and drop other Skill input fields, idempotently.

#### Scenario: Inspect skill and hub calls
Test: unit — native stream copy in transcript.rs and Rust/edge render input sanitizer.
- **WHEN** a skill or hub invocation is active or completed
- **THEN** its header names the supplied skill or specific hub operation and target

### Requirement: File cards have no separate expansion footer
Write/Edit cards SHALL end at their diff viewport without a reserved empty expansion band. The existing header SHALL retain expansion and collapse, lazy loading and rounded clipping.

#### Scenario: Inspect expanded file card
Test: none — native GPUI visual review.
- **WHEN** a user expands or collapses a file card
- **THEN** no empty footer is reserved beneath its content viewport
- **AND** header expansion and vertical scrolling remain available
