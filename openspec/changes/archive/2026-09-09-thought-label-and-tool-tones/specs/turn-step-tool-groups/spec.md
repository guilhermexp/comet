## MODIFIED Requirements

### Requirement: Operational events default to compact summaries
The transcript SHALL start tool payloads closed and reasoning disclosures open, including during streaming and inside expanded turn steps. It SHALL preserve explicit user disclosure choices and expose all recorded details by expansion. Reasoning SHALL use the fixed label Thinking while active and Thought when complete. Its content SHALL appear in the body, open by default with explicit user collapse preserved. The reasoning arrow SHALL remain visible at rest.

#### Scenario: Interleaved reasoning and tools
Test: unit — transcript disclosure defaults and content preview; native render acceptance.
- **WHEN** a turn contains repeated reasoning and ordinary multi-tool groups
- **THEN** individual tool rows display directly without group counters, while tool payloads stay closed and reasoning details start open
- **AND** opening an event reveals its existing detail content
- **AND** explicit fold choices remain authoritative

### Requirement: Native content presentation preserves semantics
The transcript SHALL present narrative, code, user attachments and subagent summaries using consistent typography while preserving native links, previews, real child status and source data. Any nonempty reasoning body SHALL remain expandable, including a short plain paragraph or text equal to the fixed state label.

#### Scenario: Reasoning contains only its title
Test: unit — reasoning body policy; none — native GPUI visual acceptance.
- **WHEN** reasoning contains one plain paragraph
- **THEN** its header shows only Thinking or Thought and the paragraph remains available through expansion
- **AND** longer or structured reasoning remains expandable

### Requirement: Compact consistent event typography
The transcript SHALL use compact event rows and regular sans typography consistent with narrative text, keeping commands monospaced and detail disclosures usable. Action labels SHALL use a stronger neutral tone than command/path/detail tokens; failure colors SHALL remain semantic.

#### Scenario: Mixed narrative and tool activity
Test: none — native GPUI visual acceptance; analytic heights covered by existing transcript unit tests.
- **WHEN** narrative, tool, reasoning and task events share a turn
- **THEN** their spacing is compact and their labels share the narrative text scale
- **AND** command text retains monospace formatting and fits without overlapping adjacent rows

### Requirement: Disclosure arrows appear on row hover
Tool and turn disclosure arrows SHALL remain hidden at rest and appear only while their own header row is hovered. Reasoning arrows SHALL stay visible at rest. Their space and position after the text SHALL remain stable in both states.

#### Scenario: Hover one event
Test: native GPUI visual acceptance.
- **WHEN** the pointer enters a tool, tool group, task or turn summary row
- **THEN** that row's disclosure arrow becomes visible without shifting its text
- **AND** other hover-only rows' arrows remain hidden and reasoning arrows remain visible

## ADDED Requirements

### Requirement: Reasoning uses the reference spiral
Reasoning SHALL replace the sparkle with a 24px native rendering of the Agent Elements spiral. It SHALL animate only while Thinking, using four fast cycles followed by two slow cycles, and disappear completely when complete. While active with reduced motion enabled, it SHALL remain static. It SHALL use the shared throttled pulse clock and SHALL not add a second spinner.

#### Scenario: Thinking settles
Test: unit — spiral cadence; none — native visual acceptance.
- **WHEN** active reasoning completes
- **THEN** the label becomes Thought and the spiral is removed, leaving no SVG/icon
- **AND** the row keeps its alignment and expandable content
