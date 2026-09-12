## ADDED Requirements

### Requirement: Expanded tool payloads use a code surface
The transcript SHALL render an expanded tool's invocation and result inside one bordered, rounded, monospaced surface with bounded height and internal scrolling. Long lines SHALL remain available through horizontal scrolling, and existing diff semantics and explicit truncation notices SHALL be preserved.

#### Scenario: A command has long output
Test: unit — payload height budget; native GPUI scroll acceptance.
- **WHEN** the expanded invocation and result exceed the code viewport
- **THEN** the transcript allocates a bounded height and the body scrolls internally
- **AND** invocation and result remain inside the same surface

#### Scenario: A short invocation has a short result
Test: unit — payload height budget; native GPUI visual acceptance.
- **WHEN** the combined content fits the code viewport
- **THEN** the surface fits its content without an empty fixed-height area

### Requirement: Disclosure arrows appear on row hover
Transcript disclosure arrows SHALL remain hidden at rest and appear only while their own header row is hovered. Their space and position after the text SHALL remain stable in both states.

#### Scenario: Hover one event
Test: native GPUI visual acceptance.
- **WHEN** the pointer enters a tool, tool group, reasoning, task or turn summary row
- **THEN** that row's disclosure arrow becomes visible without shifting its text
- **AND** other rows' arrows remain hidden
