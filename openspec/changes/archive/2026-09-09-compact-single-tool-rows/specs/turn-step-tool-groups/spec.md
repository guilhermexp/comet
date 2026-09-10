## ADDED Requirements

### Requirement: Single tools render directly
The transcript SHALL render a single tool without an additional group summary, preserving access to invocation, output and diff details. Groups containing multiple ordinary tools SHALL retain their disclosure.

#### Scenario: One tool between narrative events
Test: unit — transcript projection and group-disclosure policy; native render checked visually.
- **WHEN** a streaming or completed turn contains a group with one ordinary tool
- **THEN** the tool row is visible without a redundant group header
- **AND** its details remain independently expandable

#### Scenario: Several consecutive tools
Test: unit — transcript group-disclosure policy.
- **WHEN** a group contains multiple ordinary tools
- **THEN** its summary disclosure remains available

### Requirement: Compact consistent event typography
The transcript SHALL use compact event rows and regular sans typography consistent with narrative text, keeping commands monospaced and detail disclosures usable.

#### Scenario: Mixed narrative and tool activity
Test: none — native GPUI visual acceptance; analytic heights covered by existing transcript unit tests.
- **WHEN** narrative, tool, reasoning and task events share a turn
- **THEN** their spacing is compact and their labels share the narrative text scale
- **AND** command text retains monospace formatting and fits without overlapping adjacent rows

### Requirement: Operational events default to compact summaries
The transcript SHALL start ordinary multi-tool groups and reasoning disclosures closed, including during streaming and inside expanded turn steps. It SHALL preserve explicit user disclosure choices and expose all recorded details by expansion. Reasoning SHALL use a single-line content preview when available.

#### Scenario: Interleaved reasoning and tools
Test: unit — transcript disclosure defaults and content preview; native render acceptance.
- **WHEN** a turn contains repeated reasoning and ordinary multi-tool groups
- **THEN** each event displays a compact summary without automatically revealing nested bodies
- **AND** opening an event reveals its existing detail content
- **AND** explicit fold choices remain authoritative

## MODIFIED Requirements

### Requirement: Show tool cards inside expanded turn steps
The transcript SHALL expose compact summaries of tool groups inside expanded `TurnSteps`. Single tools SHALL remain directly visible; multi-tool groups SHALL reveal individual calls only when explicitly expanded. Recorded reasoning SHALL remain available through its own compact disclosure.

#### Scenario: A completed prefix contains several tool groups
Test: unit — deterministic transcript projection; headed GPUI smoke.
- **WHEN** the outer TurnSteps disclosure is expanded
- **THEN** each nested event is visible as a compact summary
- **AND** expanding an event reveals its recorded details

### Requirement: Preserve explicit disclosure choices
The transcript SHALL preserve explicit group and reasoning fold choices across rerenders and virtualized remounts. Default compact presentation SHALL apply to both settled and streaming events without hiding subagent links.

#### Scenario: The user expands a compact group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user expands a group that defaulted closed
- **THEN** it remains expanded across rerenders and virtualized remounts
- **AND** independent subagent links remain directly accessible

#### Scenario: The user collapses an open nested group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user collapses a nested group they previously opened
- **THEN** the group remains collapsed across rerenders and virtualized remounts
- **AND** top-level settled groups outside TurnSteps keep the compact default
- **AND** independent Reasoning and subagent events remain in their established transcript positions
