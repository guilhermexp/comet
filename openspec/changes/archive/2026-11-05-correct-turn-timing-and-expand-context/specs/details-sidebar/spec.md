# Spec Delta

## ADDED Requirements

### Requirement: Turn stats uses complete native model timing
Turn stats SHALL use native request durations for model time and its output rate when the selected runtime reports complete timing for that turn. Durations SHALL be accumulated independently of tool and wall-clock time. Partial, invalid or absent native timing SHALL display as unavailable, not as a measured wall-time-minus-tools estimate.

#### Scenario: Native OMP duration determines the rate
Test: harness normalization and engine unit regression; real-runtime smoke through the harness.
- **WHEN** a turn has 13,824 output tokens and native request durations totaling 143,607.892 milliseconds
- **THEN** model time is approximately 143.6 seconds and output rate is approximately 96.3 tokens per second
- **AND** the whole-turn rate continues to use the turn's own elapsed time
- **AND** concurrent tools and child subagent work do not inflate the parent's native model time

#### Scenario: Missing, invalid and zero durations
Test: harness and engine unit regressions; UI value projection.
- **WHEN** any timing contribution needed for the turn is absent, negative, nonfinite or unrepresentable
- **THEN** model time and model output rate display unavailable while token, cache and cost totals remain usable
- **WHEN** the runtime reports a valid zero duration
- **THEN** zero remains distinct from missing and no division-by-zero rate is displayed

### Requirement: Session Context expands inline
The Session card SHALL retain its compact Context row and usage bar by default and provide an accessible expand/collapse control. Expanding SHALL reveal the selected session's context budget directly below that row. Expansion state SHALL be scoped to the selected Details context and SHALL not alter Project, Turn stats or Context sources behavior.

#### Scenario: Expand and collapse the existing indicator
Test: UI state/interaction tests; native appearance requires visual acceptance under the project's isolation policy.
- **WHEN** the user activates the Context expansion control by pointer or keyboard
- **THEN** the inline context budget opens with the current used tokens, capacity and percentage
- **AND** a second activation restores the compact presentation
- **AND** the control exposes an accessible name and expanded state

#### Scenario: Preserve selection and layout
Test: sidebar preference round-trip and stale-response tests; native light/dark and narrow-width review.
- **WHEN** the user switches Chats or restores sidebar preferences
- **THEN** the matching context's expansion choice is restored
- **AND** an outstanding response from another Chat or host cannot replace the selected breakdown
- **AND** long labels and paths do not overlap the token and percentage columns

### Requirement: Context budget distinguishes active reserved free and deferred material
The expanded view SHALL show a segmented budget bar and category rows with available token counts and percentages of the native capacity. Active material, autocompact reserve, free space and deferred tools SHALL remain distinct. Deferred tools SHALL not consume active-budget segments or percentages. Native groups and attribution limits SHALL be preserved without double counting.

#### Scenario: Display the requested breakdown categories
Test: shared budget projection with provider fixtures; native render acceptance.
- **WHEN** the runtime supplies category attribution
- **THEN** the view presents messages, system tools, skills, system prompt, memory files, MCP server instructions, autocompact buffer, free space, deferred system tools and deferred MCP tools as applicable
- **AND** any additional native grouping, such as System context or active MCP tools, remains explicit rather than silently reassigned
- **AND** each displayed percentage uses the same reported context capacity

#### Scenario: Deferred tools stay outside active usage
Test: budget projection boundary tests.
- **WHEN** active usage is 828,300 tokens, reserve is 33,000 tokens, capacity is 1,000,000 tokens and deferred tools have nonzero counts
- **THEN** deferred rows show no active-budget percentage and do not increase active usage
- **AND** reserve and free space are not counted as active messages
- **AND** free space is derived only where the runtime contract supports that calculation

### Requirement: Expanded context includes native item details
The expanded budget SHALL provide independently expandable MCP-tool and memory-file sections when their runtime-native lists are available, with item names or paths and available token attribution. Unsupported lists SHALL be distinguished from measured empty lists. The panel SHALL not expose prompt bodies, tool-result contents, memory contents or credentials.

#### Scenario: Inspect MCP tools and memory files
Test: normalized runtime payload fixtures and UI section-state projection; native interaction acceptance.
- **WHEN** the runtime reports tools and memory files
- **THEN** each section shows the reported count and expands to bounded-scroll item rows
- **AND** deferred or unloaded tool status is preserved
- **AND** missing per-item tokens do not become zeros or fabricated shares of an aggregate

### Requirement: Context availability and freshness are explicit
The panel SHALL distinguish loading, current, stale, partial, unsupported and failed inspection. It SHALL show the snapshot's observation time and mark native estimates as estimates. Real zero values SHALL remain distinct from missing attribution. A failed refresh SHALL not turn prior measurements into fresh zero values.

#### Scenario: Different runtimes provide different detail
Test: OMP, Claude and Codex payload projections and older-runtime error paths.
- **WHEN** Claude supplies a detailed snapshot, the extended OMP supplies its native breakdown, or Codex supplies only total usage
- **THEN** the view displays each runtime's real available data
- **AND** unreported or inapplicable categories are explained rather than fabricated
- **AND** an OMP version without structured inspection reports that a runtime update is required

#### Scenario: Age and unavailable hosts
Test: engine snapshot lifecycle, remote forwarding and UI generation tests.
- **WHEN** no newer measurement exists, the runtime configuration changes, or the session's host becomes unavailable
- **THEN** the last snapshot remains labeled with its actual observation time and appropriate stale/unavailable state
- **AND** the UI does not claim another host's or another model's snapshot is current

### Requirement: Context inspection is local read only and host scoped
Opening or refreshing the expanded view SHALL use the selected Chat's host runtime and SHALL not send a conversation prompt, create an assistant turn, invoke a provider token-count API, rebuild the provider prompt or mutate its tools and cache prefix. Updates SHALL follow native events and explicit inspection, not a background polling loop.

#### Scenario: Inspection does not change the conversation
Test: runtime protocol integration and live read-only smoke; cache-prefix/state comparison.
- **WHEN** the user opens and refreshes the Context budget
- **THEN** no model request or visible conversation entry is created
- **AND** the runtime's prompt digest, tools, messages and usage totals are unchanged
- **AND** the response is scoped to the selected Chat and target device
- **AND** unsupported inspection never falls back to a fresh unrelated runtime or a paid diagnostic prompt
