# turn-step-tool-groups Specification

## Purpose

Keep every nested tool card visible when an assistant turn's operational steps
are expanded, without opening each card's invocation, output, or diff details.

## Requirements

### Requirement: Show tool cards inside expanded turn steps

The transcript SHALL show the individual cards of every tool group projected inside an expanded `TurnSteps` disclosure by default and SHALL place completed Thinking that belongs to those operations inside the same disclosure as rendered Markdown.

#### Scenario: A completed prefix contains several tool groups

Test: deterministic transcript projection test plus headed GPUI smoke.

- **WHEN** completed Thought, Thinking, and tool activity is projected into `TurnSteps`
- **AND** the outer disclosure is expanded
- **THEN** each nested `Ran`, `Read`, `Edit`, or generic tool group shows its individual cards
- **AND** associated Thinking is readable as Markdown within the operational context
- **AND** the transcript does not require the user to expand every group manually

### Requirement: Keep card details independently compact

The transcript SHALL keep invocation, output, and diff detail bodies closed by
default when it opens a nested tool group.

#### Scenario: A completed command card has recorded output

Test: deterministic projection test asserting independent group and detail defaults.

- **WHEN** a completed command group becomes a `TurnSteps` child
- **THEN** the command card is visible
- **AND** its output and invocation bodies remain closed until the user opens the card

### Requirement: Preserve explicit disclosure choices

The transcript SHALL keep explicit group fold state authoritative over the default and SHALL preserve existing top-level, first-class Reasoning, subagent, and streaming behavior.

#### Scenario: The user collapses an open nested group

Test: fold-state precedence unit coverage and virtualized remount regression suite.

- **WHEN** the user collapses a nested group that defaulted open
- **THEN** the group remains collapsed across rerenders and virtualized remounts
- **AND** top-level settled groups outside `TurnSteps` keep their existing default
- **AND** independent Reasoning and subagent cards remain in their established transcript positions
