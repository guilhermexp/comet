# Spec Delta

## MODIFIED Requirements

### Requirement: Context availability and freshness are explicit

The panel SHALL distinguish loading, current, stale, partial, unsupported and failed inspection without discarding valid aggregate context facts when composition is unavailable. Snapshot time and estimate attribution SHALL be accessible through compact contextual help rather than persistent technical paragraphs. Real zero values SHALL remain distinct from missing attribution. A failed refresh SHALL not turn prior measurements into fresh zero values.

#### Scenario: Different runtimes provide different detail
Test: OMP, Claude, Codex and Pi payload projections and older-runtime error paths.

- **WHEN** Claude supplies a detailed snapshot, a compatible OMP producer supplies its native breakdown, or another runtime supplies only total usage
- **THEN** the view displays each runtime's real available data
- **AND** unreported or inapplicable categories are distinguished from measured zero without fabricated attribution
- **AND** an OMP build without structured inspection identifies the missing capability without claiming that an ordinary official update necessarily provides it

#### Scenario: Age and unavailable hosts
Test: engine snapshot lifecycle, remote forwarding and UI generation tests.

- **WHEN** no newer measurement exists, the runtime configuration changes, or the session's host becomes unavailable
- **THEN** the last snapshot retains its actual observation time and appropriate stale/unavailable state
- **AND** observation time is accessible from the compact freshness/help affordance
- **AND** the UI does not claim another host's or another model's snapshot is current

## ADDED Requirements

### Requirement: Aggregate context remains usable without native composition

The expanded Context SHALL display valid native used tokens and capacity available for the selected Chat even when its runtime has no detailed inspection API. Aggregate-only data SHALL be identified as partial detail. Totals SHALL retain their provenance and freshness, remain consistent with the compact indicator and SHALL NOT be replaced by cumulative billing usage, estimates from transcript length or another runtime's data.

#### Scenario: Codex or Pi reports totals without composition
Test: harness normalization and engine integration with runtime context fixtures; UI value projection.

- **WHEN** the selected runtime has reported context usage and capacity but its detailed-inspection path is unsupported
- **THEN** expansion shows those totals and the corresponding aggregate usage bar instead of empty `Used / capacity` values
- **AND** it does not claim category, tool or memory attribution that the runtime did not report
- **AND** a missing capacity or a valid zero remains distinguishable from an unmeasured total

#### Scenario: OMP official build lacks the detailed producer
Test: OMP subprocess fixture with supported aggregate state and Unknown command for detailed inspection.

- **WHEN** an OMP build supplies native aggregate context but rejects the detailed query
- **THEN** the aggregate remains visible as partial detail and the known missing capability is accessible through concise contextual help
- **AND** repeated refresh does not create a prompt, install a different executable or rebuild the provider request

#### Scenario: Prior data belongs to a different runtime or model
Test: engine runtime/configuration lifecycle integration.

- **WHEN** aggregate or detailed data was measured for a previous host, runtime or selected model and cannot be validated for the current selection
- **THEN** it is not relabeled as a current measurement for the selected runtime
- **AND** any retained historical values preserve their stale/provenance distinction

### Requirement: Context body prioritizes measured data and concise state

The Context body SHALL omit persistent source, observation-time, unknown-measurement-time and raw-protocol diagnostic paragraphs. It SHALL present one concise state when needed and only usable data sections. Technical provenance and actionable failure details SHALL remain available through contextual help. Unsupported lists SHALL NOT appear as empty inventories, and an empty report SHALL NOT render a meaningless capacity bar.

#### Scenario: Empty unsupported report is concise
Test: UI derivation and GPUI render fixture; native visual acceptance under the project's isolation policy.

- **WHEN** neither aggregate measurement nor detailed data is available
- **THEN** the body shows a concise unavailable state with the applicable action or explanation under interaction
- **AND** it omits the empty capacity bar and unreported MCP/memory sections
- **AND** it does not append `Native runtime`, `Observed …`, `Provider measurement time unknown` or a raw runtime refusal as body paragraphs

#### Scenario: Complete and partial reports remain readable
Test: UI render and interaction fixtures with narrow/normal widths and existing themes.

- **WHEN** complete or partial native context facts are available
- **THEN** the main body contains measured values, supported categories and independently expandable native lists
- **AND** metadata and precision explanations do not become a paragraph stack below those lists
- **AND** measured empty lists remain distinguishable from unsupported lists without adding fictitious entries or zero token attribution

### Requirement: Detailed runtime support requires producer and consumer evidence

The system SHALL claim complete context composition only when the selected runtime build supplies the native contract and Comet consumes it successfully. Source-only producer tests SHALL NOT be represented as support in the installed official CLI. Available catalogs or discovered files SHALL NOT be labeled as loaded context without runtime evidence.

#### Scenario: Coordinated OMP producer is present
Test: read-only producer/consumer contract integration with a verified compatible OMP build.

- **WHEN** a compatible OMP build exposes the agreed structured snapshot for the selected live Chat
- **THEN** Comet presents its supported categories, attribution and item lists, preserving native completeness limits
- **AND** the verification records the actual executable/build and consumer revision without changing global runtime configuration

#### Scenario: Runtime supplies metadata without context attribution
Test: capability and normalized-payload fixtures.

- **WHEN** a runtime exposes tool catalogs, file discovery or transcript events but no attribution of their presence in active context
- **THEN** those inventories are not substituted for measured context composition
- **AND** the Context detail remains explicitly partial instead of claiming a complete CLI-context dump
