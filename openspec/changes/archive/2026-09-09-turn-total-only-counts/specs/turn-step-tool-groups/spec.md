## MODIFIED Requirements

### Requirement: Show tool cards inside expanded turn steps
The transcript SHALL expose individual tool rows directly inside expanded `TurnSteps`, without intermediate count headers. Recorded reasoning SHALL remain available through its own compact disclosure.

#### Scenario: A completed prefix contains several tool groups
Test: unit — deterministic transcript projection; headed GPUI smoke.
- **WHEN** the outer TurnSteps disclosure is expanded
- **THEN** each individual call is visible directly without a group counter
- **AND** expanding an event reveals its recorded details

### Requirement: Preserve explicit disclosure choices
The transcript SHALL preserve explicit turn, tool-detail and reasoning fold choices across rerenders and virtualized remounts. Default compact presentation SHALL apply to both settled and streaming events without hiding subagent links.

#### Scenario: The user expands a compact group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user expands the completed turn summary that defaulted closed
- **THEN** it remains expanded across rerenders and virtualized remounts
- **AND** independent subagent links remain directly accessible

#### Scenario: The user collapses an open nested group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user collapses a tool detail they previously opened
- **THEN** the group remains collapsed across rerenders and virtualized remounts
- **AND** top-level tool calls outside TurnSteps remain directly visible without aggregate headers
- **AND** independent Reasoning and subagent events remain in their established transcript positions

### Requirement: Single tools render directly
The transcript SHALL render a single tool without an additional group summary, preserving access to invocation, output and diff details. Groups containing multiple ordinary tools SHALL also render their calls directly without aggregate disclosures.

#### Scenario: One tool between narrative events
Test: unit — transcript projection and group-disclosure policy; native render checked visually.
- **WHEN** a streaming or completed turn contains a group with one ordinary tool
- **THEN** the tool row is visible without a redundant group header
- **AND** its details remain independently expandable

#### Scenario: Several consecutive tools
Test: unit — transcript group-disclosure policy.
- **WHEN** a group contains multiple ordinary tools
- **THEN** every call is directly visible and only the completed TurnSteps summary displays aggregate counts

### Requirement: Operational events default to compact summaries
The transcript SHALL start tool payloads and reasoning disclosures closed, including during streaming and inside expanded turn steps. It SHALL preserve explicit user disclosure choices and expose all recorded details by expansion. Reasoning SHALL use a single-line content preview when available.

#### Scenario: Interleaved reasoning and tools
Test: unit — transcript disclosure defaults and content preview; native render acceptance.
- **WHEN** a turn contains repeated reasoning and ordinary multi-tool groups
- **THEN** individual tool rows display directly without group counters, while their payloads and reasoning details stay closed
- **AND** opening an event reveals its existing detail content
- **AND** explicit fold choices remain authoritative
