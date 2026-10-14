# Spec Delta

## ADDED Requirements

### Requirement: Ordinary tool runs fold into a tree group
The transcript SHALL render each run of consecutive ordinary tools as one group with a collapsible summary header that counts the calls by kind and failures (for example "Ran 6 commands · 1 failed"). Beneath an open header each call SHALL render as a child row connected by tree branches: a trunk under the header icon, a rounded elbow and a branch to each child. Connector branches SHALL reveal with each arriving row and SHALL appear complete under reduced motion. The group that a streaming turn is writing into SHALL default open; finished groups and groups inside TurnSteps SHALL default collapsed. An explicit toggle SHALL persist across rerenders and virtualized remounts. Agent and spawn links SHALL never fold behind a header. File Write/Edit calls SHALL be rows of the same group and SHALL NOT split a run of tools.

#### Scenario: A live turn runs several commands and edits a file
Test: unit — transcript projection (one group, summary, live default open); none — native GPUI visual acceptance of connectors.
- **WHEN** a streaming turn runs commands, edits a file and runs more commands without narrative between them
- **THEN** one group shows a summary header with every call, the edit included, as tree children
- **AND** the group stays open while the turn writes into it

#### Scenario: A finished group
Test: unit — group disclosure defaults and fold precedence.
- **WHEN** the turn moves past a group or completes
- **THEN** the group shows only its summary header until the user opens it
- **AND** a group the user opened or closed keeps that choice

## MODIFIED Requirements

### Requirement: Show tool cards inside expanded turn steps
The transcript SHALL expose the turn's tool groups inside expanded `TurnSteps`, each behind its summary header; opening a group reveals its individual tool rows. Recorded reasoning SHALL remain available through its own compact disclosure.

#### Scenario: A completed prefix contains several tool groups
Test: unit — deterministic transcript projection; headed GPUI smoke.
- **WHEN** the outer TurnSteps disclosure is expanded
- **THEN** each tool group shows its summary header, collapsed by default
- **AND** opening a group shows its calls, and expanding a call reveals its recorded details

### Requirement: Preserve explicit disclosure choices
The transcript SHALL preserve explicit turn, tool-group, tool-detail and reasoning fold choices across rerenders and virtualized remounts. Default compact presentation SHALL apply to both settled and streaming events without hiding subagent links.

#### Scenario: The user expands a compact group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user expands the completed turn summary that defaulted closed
- **THEN** it remains expanded across rerenders and virtualized remounts
- **AND** independent subagent links remain directly accessible

#### Scenario: The user collapses an open nested group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user collapses a tool group or a tool detail they previously opened
- **THEN** it remains collapsed across rerenders and virtualized remounts
- **AND** independent Reasoning and subagent events remain in their established transcript positions

### Requirement: Operational events default to compact summaries
The transcript SHALL start tool payloads closed and reasoning disclosures open, including during streaming and inside expanded turn steps; tool groups follow the live-open, finished-collapsed default. It SHALL preserve explicit user disclosure choices and expose all recorded details by expansion. Reasoning SHALL use the fixed label Thinking while active and Thought when complete. Its content SHALL appear in the body, open by default with explicit user collapse preserved. The reasoning arrow SHALL remain visible at rest.

#### Scenario: Interleaved reasoning and tools
Test: unit — transcript disclosure defaults and content preview; native render acceptance.
- **WHEN** a turn contains repeated reasoning and ordinary multi-tool groups
- **THEN** each tool run displays behind its group summary, while tool payloads stay closed and reasoning details start open
- **AND** opening an event reveals its existing detail content
- **AND** explicit fold choices remain authoritative

### Requirement: File edits show a bounded live typing preview
A Write/Edit row SHALL show filename shimmer and a spinner while its content is generated, and line statistics once complete. Opening the row SHALL show its diff card: progressive generated content in a 72px viewport with at most 15 recent lines, bottom aligned after three lines, while active; the authoritative diff, expandable up to 200px with vertical scrolling, once complete. Text SHALL wrap with no horizontal scrolling. Partial input refreshes SHALL be time gated at 100ms after initial semantic previews, preserving bounded decoding and final content delivery. Active previews SHALL omit syntax highlighting; completion SHALL request syntax highlighting after 50ms. Errors SHALL remain visible.

#### Scenario: Small incremental input chunks
Test: unit — harness partial input decoder.
- **WHEN** a file tool streams small chunks beyond the initial preview
- **THEN** its preview refreshes after 100ms without requiring 16KB of new input
- **AND** final flush preserves the last decoded content

#### Scenario: Compact generated tail
Test: unit — ui file_change projection; none — native GPUI layout and shimmer.
- **WHEN** an opened Write or Edit row generates more than three lines
- **THEN** at most 15 generated lines are bottom aligned in a 72px viewport
- **AND** completion restores the authoritative diff and permits expansion to 200px

## REMOVED Requirements

### Requirement: Single tools render directly
**Reason**: Upstream's tree organization folds every run of ordinary tools behind a summary header; the owner chose it over flat rows.
**Migration**: Tool calls are rows of their group; open the group to see them.
