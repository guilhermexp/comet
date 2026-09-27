# user-message-presentation Specification

## Purpose

Present user messages in the native Chat as readable inline bubbles matching upstream, without obscuring the conversation during scrolling.

## Requirements

### Requirement: User messages scroll in the conversation

The native transcript SHALL display user messages as right-aligned bubbles at no more than 80% of the conversation column. Messages SHALL scroll out of view with their rows, without a pinned duplicate or continuation blur.

#### Scenario: Read an assistant response below a prompt

Test: none — native render QA per crates/ui/AGENTS.md, including opaque and frosted surfaces.

- **WHEN** the user scrolls the prompt above the viewport
- **THEN** the prompt leaves the screen and no fixed user card covers the assistant response

### Requirement: Long messages expand inline

The transcript SHALL collapse long user text to five rendered lines followed by a separate ellipsis and Show more control. Expansion SHALL reveal the full text with Show less, without changing durable content or opening a sticky-message dialog.

#### Scenario: Expand and collapse a long prompt

Test: unit — collapse and fold-state regressions; native QA for actual wrapping and controls.

- **WHEN** a prompt exceeds five lines at the current width
- **THEN** its collapsed bubble shows five lines, an ellipsis and Show more
- **AND** Show more expands the prompt and Show less restores the collapsed presentation

#### Scenario: Short and rich prompts

Test: unit — existing row projection, mention, URL and attachment regressions; native QA for appearance.

- **WHEN** a short prompt contains supported links, mentions, badges or attachments
- **THEN** it retains its content and existing actions without an unnecessary expansion control
- **AND** pending messages retain their pending appearance

### Requirement: Preserve own-turn arrival and reading position

The transcript SHALL retain smooth own-send arrival and response-space reservation. Manual scrolling and message expansion SHALL preserve user control over reading position.

#### Scenario: Send or expand a prompt

Test: unit — runway, viewport and fold regression tests; native QA for expansion during scrolling.

- **WHEN** a local send arrives
- **THEN** the original user row lands at the existing top inset and reserves response space
- **AND** manual scrolling releases the hold without creating a sticky copy
- **AND** expanding an older message does not jump the view to the bottom
