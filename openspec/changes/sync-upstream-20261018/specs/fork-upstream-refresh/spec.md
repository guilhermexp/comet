## ADDED Requirements

### Requirement: Preserve the fork while integrating pinned upstream
The integration SHALL incorporate U1–U11 through c526d6f7 by real merge while retaining fork grants, Workers ownership, existing UI contracts, release feed and version 1.0.7.

#### Scenario: U1 Native sandboxed Luau visuals
- **WHEN** the integrated U1 path is exercised
- **THEN** it SHALL provide Native sandboxed Luau visuals, controls/charts/tables, 2D/3D/shaders/assets with bounded validation, off-main GPU, reuse and teardown; root-only MCP grants
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U2 zatex inline/display math
- **WHEN** the integrated U2 path is exercised
- **THEN** it SHALL provide zatex inline/display math, six fonts and persisted settings, bounded parser/layout and streaming fallback; retain file links/Mermaid/selection
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U3 Side-chat/subagent lanes
- **WHEN** the integrated U3 path is exercised
- **THEN** it SHALL provide Side-chat/subagent lanes, folded finished rows, bounded scroll/counts/rename/harness marks and checkout totals; reconcile Details Workers ownership and existing new/fork buttons without duplicate activity
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U4 Convergent section header drag order
- **WHEN** the integrated U4 path is exercised
- **THEN** it SHALL provide Convergent section header drag order, iOS cross-section/pin drag, neighbor anchors and advertised capability
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U5 Dwell/warm hover card
- **WHEN** the integrated U5 path is exercised
- **THEN** it SHALL provide Dwell/warm hover card, dismiss on scroll/drag/menu, once-per-frame lifecycle and metadata; retain mentions and fork rows
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U6 OpenCode v2 forms normalize to field-keyed input panels and form route; v1 works and engine owns lifecycle
- **WHEN** the integrated U6 path is exercised
- **THEN** it SHALL provide OpenCode v2 forms normalize to field-keyed input panels and form route; v1 works and engine owns lifecycle
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U7 Cursor Plus catalog/defaults on fresh and switched runs
- **WHEN** the integrated U7 path is exercised
- **THEN** it SHALL provide Cursor Plus catalog/defaults on fresh and switched runs, warm catalog; protect dev bundles with fork updater/feed
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U8 Recent Files flicker fix with unified search/index ownership
- **WHEN** the integrated U8 path is exercised
- **THEN** it SHALL provide Recent Files flicker fix with unified search/index ownership
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U9 Persist sidebar Archived visibility setting
- **WHEN** the integrated U9 path is exercised
- **THEN** it SHALL provide Persist sidebar Archived visibility setting
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U10 Bulk archived Chat deletion with explicit confirmation and device/sync routing
- **WHEN** the integrated U10 path is exercised
- **THEN** it SHALL provide Bulk archived Chat deletion with explicit confirmation and device/sync routing
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: unit

#### Scenario: U11 Adaptive iOS split/tab shell
- **WHEN** the integrated U11 path is exercised
- **THEN** it SHALL provide Adaptive iOS split/tab shell, fold state, uneven safe areas, transcript/composer alignment; native acceptance pending
- **AND** its evidence SHALL distinguish Linux regression results from native platform acceptance

Test: e2e

#### Scenario: Visual publication keeps root grants and bounded validation
- **WHEN** a root Chat receives its authorized tools and publishes a visual during a live turn
- **THEN** the engine SHALL validate bounded script/assets before storing and publishing atomic references
- **AND** child Chats, subagents, Workers and internal title/recap runs SHALL not gain the root tool grant
- **AND** invalid scripts or absent live turns SHALL fail explicitly without a persisted visual

Test: integration

#### Scenario: Math retains source and rendering budgets
- **WHEN** formulas stream, fail to parse, exceed layout budgets or switch among the six fonts
- **THEN** desktop/mobile SHALL preserve readable source fallback and source text for copy/selection
- **AND** settings changes SHALL invalidate each surface's measured formula rows without replacing file links or Mermaid

Test: unit

#### Scenario: OpenCode question forms keep engine input ownership
- **WHEN** v2 emits a question form with keyed single and multiple selection fields
- **THEN** the real adapter SHALL submit the field-keyed answer body to the form route
- **AND** the adapter SHALL emit no duplicate input lifecycle events
- **AND** v1 question request/reply paths SHALL continue to work

Test: integration

#### Scenario: Cursor defaults follow the current model
- **WHEN** a run starts or changes model with implicit model parameters
- **THEN** the real adapter SHALL fill options from the credential-scoped warm catalog without waiting for an unnecessary refresh
- **AND** explicit options SHALL win and rejected Max Mode defaults SHALL be excluded

Test: integration

#### Scenario: Fork UI state remains authoritative
- **WHEN** Details hides or yields its subagent presentation, a section moves, or Archived visibility changes
- **THEN** lanes SHALL retain bounded side-chat/subagent navigation and cancellation state without duplicate activity
- **AND** hidden metadata SHALL release scoped diff watches before route/gate early exits
- **AND** the central settings writer SHALL persist Archived visibility without clobbering settings owned elsewhere

Test: unit

#### Scenario: Native visual traversal and materialization are bounded
- **WHEN** scripts reuse empty/false lists or strings, or models repeat image/buffer references
- **THEN** native traversal SHALL consume bounded work and enforce its deadline independently from Lua interrupts
- **AND** text/log/model aggregate byte and entry budgets SHALL be checked before copying or decoding
- **AND** independent retained default copies, numeric children, canvas text and scene axes/default labels SHALL consume the same pre-copy byte budgets
- **AND** legitimate nested lists, numeric text, logs and repeated image views SHALL remain usable

Test: integration

#### Scenario: Grouped iOS Chats remain draggable
- **WHEN** the front page groups unassigned Chats by Project
- **THEN** those rows SHALL remain drag sources for pins and custom sections
- **AND** project groups SHALL remain disallowed destinations and archive/search restrictions SHALL remain

Test: unit

#### Scenario: Cursor rejected discovery preserves coherent defaults
- **WHEN** discovery refresh is empty, invalid, concurrent or changes credentials
- **THEN** models and Max Mode exclusions SHALL come from the same accepted snapshot
- **AND** start and switch SHALL omit incompatible implicit defaults while explicit overrides remain intact

Test: integration
