## MODIFIED Requirements

### Requirement: Operational events default to compact summaries
The transcript SHALL start tool payloads closed and reasoning disclosures open inside expanded turn steps and in settled turns. While an assistant entry is streaming, command payloads — exec, write, edit and patch calls — SHALL start open in every group of that entry, including calls that already finished, and SHALL return to the closed default when the entry settles. Other payloads SHALL start closed in both phases. The transcript SHALL preserve explicit user disclosure choices and expose all recorded details by expansion. Reasoning SHALL use the fixed label Thinking while active and Thought when complete. Its content SHALL appear in the body, open by default with explicit user collapse preserved. The reasoning arrow SHALL remain visible at rest.

#### Scenario: Interleaved reasoning and tools
Test: unit — transcript disclosure defaults and content preview; native render acceptance.
- **WHEN** a settled turn contains repeated reasoning and ordinary multi-tool groups
- **THEN** individual tool rows display directly without group counters, while tool payloads stay closed and reasoning details start open
- **AND** opening an event reveals its existing detail content
- **AND** explicit fold choices remain authoritative

#### Scenario: Commands run inside a live turn
Test: unit — transcript projection and detail defaults.
- **WHEN** an assistant entry is streaming and its groups contain command, write, edit or patch calls
- **THEN** each of those payloads renders open without a click, in the trailing group and in earlier ones alike
- **AND** read, search, MCP and other payloads remain closed

#### Scenario: The live turn settles
Test: unit — transcript projection and detail defaults.
- **WHEN** the streaming entry reaches a settled status
- **THEN** the command payloads that had opened automatically return to closed
- **AND** a payload the user opened or closed by hand keeps that choice

### Requirement: Preserve explicit disclosure choices
The transcript SHALL preserve explicit turn, tool-detail and reasoning fold choices across rerenders and virtualized remounts. Default compact presentation SHALL apply to settled events, and to streaming events other than command payloads, without hiding subagent links.

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

### Requirement: File edits show a bounded live typing preview
Write/Edit cards SHALL display progressive generated content with at most 15 recent lines, bottom aligned after three lines. While the owning entry is streaming, the card body SHALL use the expanded 200px budget in both the generating and the settled-call states, so its height does not change as the call resolves; once the entry settles, the card SHALL return to the 72px collapsed viewport. Text SHALL wrap with no horizontal scrolling. Partial input refreshes SHALL be time gated at 100ms after initial semantic previews, preserving bounded decoding and final content delivery. Active cards SHALL omit syntax highlighting and show filename shimmer and a spinner. Completion SHALL replace activity with line statistics and expansion affordances, requesting syntax highlighting after 50ms. Header and collapsed body SHALL expand the final file up to 200px with vertical scrolling. Errors SHALL remain visible.

#### Scenario: Small incremental input chunks
Test: unit — harness partial input decoder.
- **WHEN** a file tool streams small chunks beyond the initial preview
- **THEN** its preview refreshes after 100ms without requiring 16KB of new input
- **AND** final flush preserves the last decoded content

#### Scenario: Compact generated tail
Test: unit — ui file_change projection; none — native GPUI layout and shimmer.
- **WHEN** Write or Edit generates more than three lines
- **THEN** at most 15 generated lines are bottom aligned in the card's viewport
- **AND** completion restores the authoritative diff and permits expansion to 200px

#### Scenario: A file edit finishes inside a live turn
Test: unit — transcript projection; none — native GPUI height acceptance.
- **WHEN** a Write or Edit call resolves while its entry is still streaming
- **THEN** its diff is visible at the expanded budget without a click, from the doc-resident preview
- **AND** the full-file fetch is not started on the card's behalf
- **AND** a preview cut short still shows its earlier-lines notice

#### Scenario: The turn holding a file edit settles
Test: unit — transcript projection.
- **WHEN** the streaming entry reaches a settled status
- **THEN** the file card returns to its 72px collapsed preview
- **AND** a card the user opened or closed by hand keeps that choice
