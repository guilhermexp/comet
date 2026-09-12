## MODIFIED Requirements

### Requirement: Ordered multi-run event model

Each captured record MUST preserve stable ordering, observed event time when available, run identity, semantic kind, status, error state, and available turn, step, call, duration, usage, and correlation data. The preview MUST combine every run for the selected Chat captured on the current device and MUST render explicit run boundaries. Observed input consumption SHALL delimit user Turns; observed model responses with their tools SHALL delimit assistant Steps. Unknown boundaries MUST remain explicitly unknown. Boundary replay MUST NOT create duplicate groups or user content.

#### Scenario: Multiple local runs
Test: integration
- **GIVEN** two runs for one Chat were captured on the current device
- **WHEN** Trajectory opens
- **THEN** both runs appear in recorded order with an explicit boundary and independent run state

#### Scenario: Incomplete run or call
Test: unit
- **GIVEN** a recovered run ended without a terminal event or a tool call has no result
- **WHEN** Trajectory renders it
- **THEN** the run is marked interrupted or the call is marked unsettled
- **AND** no completion time or result is fabricated

#### Scenario: Consumed input and model responses
Test: integration
- **GIVEN** one consumed user input produces two model responses and their tools
- **WHEN** the trajectory is captured
- **THEN** the responses occupy two Steps within that input's Turn
- **AND** a later consumed input opens a new Turn exactly once

#### Scenario: Queued steering is not consumed input
Test: integration
- **WHEN** steering is acknowledged or queued but not consumed
- **THEN** no consumed-input Turn is fabricated
- **WHEN** the runtime later consumes that input
- **THEN** the boundary is captured once without duplicating transcript content

#### Scenario: Boundary source is unavailable
Test: unit
- **WHEN** the source does not expose enough information to delimit a Turn or Step
- **THEN** the missing boundary is labeled unknown rather than presented as a measured numbered group

### Requirement: Honest legacy projection

Eligible local legacy journal history MUST be imported idempotently. Events without per-event timestamps MUST use sequence geometry, and Duration and Timing MUST be unavailable for those records. Unknown run boundaries MUST NOT be invented. Existing history SHALL gain operation correlation and recoverable source facts without destructive reimport, loss of existing records, or recovery-journal rewriting. Missing historical original arguments, schemas and measurements MUST remain explicitly not captured or unavailable.

#### Scenario: Open timestamp-free legacy history
Test: integration
- **GIVEN** a legacy local journal has ordered events but no per-event timestamps
- **WHEN** the Chat's Trajectory is opened or imported
- **THEN** the events appear once in sequence order
- **AND** affected Duration and Timing values are unavailable rather than zero or estimated
- **AND** unknowable boundaries are represented as one labeled legacy run

#### Scenario: Legacy corrupt tail
Test: integration
- **GIVEN** a legacy journal has a valid prefix and corrupt trailing content
- **WHEN** it is projected
- **THEN** only the valid prefix is imported
- **AND** the incomplete remainder is represented honestly

#### Scenario: Recover context without inventing token consumption
Test: integration
- **GIVEN** historical source records context occupancy 109686 with window 272000 and only synthetic adapter zeros for consumption
- **WHEN** historical enrichment runs
- **THEN** the occupancy remains 109686 of 272000
- **AND** consumption is unknown rather than reported as measured zero
- **AND** valid measured zeros from other sources are not rewritten

#### Scenario: Resume interrupted enrichment
Test: integration
- **WHEN** enrichment is interrupted and later retried while native capture continues
- **THEN** source events and recovered facts remain idempotent
- **AND** newer native information is not replaced by older enrichment
- **AND** existing history remains readable if an eligible source is missing or corrupt
- **AND** recovery journals remain unchanged

#### Scenario: Historical source did not retain original fields
Test: integration
- **WHEN** an old operation has no retained original arguments or schema snapshot
- **THEN** those fields are reported as not captured
- **AND** the current tool catalog or normalized arguments are not substituted as original historical data

### Requirement: Coherent history and live updates

Opening historical data and following a live run MUST converge on one stable ordering and projection contract. Records delivered across the history-to-live boundary MUST appear exactly once, and missing live ranges MUST trigger an explicit degraded or resnapshot state rather than silent reordering. Reconnection SHALL receive committed revisions to already-delivered positions as well as new positions. An older snapshot or query response MUST NOT overwrite a newer operation outcome or another selection.

#### Scenario: Event occurs while opening
Test: integration
- **WHEN** a new event is captured while a Trajectory surface establishes its historical view
- **THEN** the event appears exactly once in the resulting ordered view

#### Scenario: Reconnect after a watermark
Test: integration
- **GIVEN** a surface has already received records through a known watermark
- **WHEN** its live watch reconnects
- **THEN** missing later records and newer committed revisions to earlier positions are applied
- **AND** duplicate delivery does not create duplicate rows

#### Scenario: Updated old position after reconnection
Test: integration
- **GIVEN** a record at an already-delivered position receives a later committed revision
- **WHEN** the client reconnects from its prior position and revision
- **THEN** it receives the update once without duplicating the event row

#### Scenario: Completion races an operation lookup
Test: integration
- **WHEN** a newer completion arrives while a historical operation query is in flight
- **THEN** the older query response cannot regress the visible operation to Running
- **AND** a response for a previous selection cannot replace the current inspector

### Requirement: Duration, Turns, Calls, and Search controls

The toolbar MUST provide Duration, Turns, Calls, and Search controls. Duration MUST switch between equal-width sequence geometry and recorded geometry for observed instants and intervals without presenting missing timing as measured data. Turns MUST fold turns independently from Calls folding tool calls under assistant steps. Calls folding MUST preserve interleaved model text and reasoning. Search and range focus MUST de-emphasize nonmatching records without removing chronological context. A sequence-only segment MUST NOT remove measured geometry from other segments or runs. Runtime execution duration and host-observed interval MUST be distinguished.

#### Scenario: Independent folding
Test: unit
- **WHEN** the user folds Turns and leaves Calls expanded
- **THEN** collapsible turns fold without changing Call fold state
- **WHEN** the user then folds Calls
- **THEN** tool calls fold without changing Turn fold state

#### Scenario: Switch duration geometry
Test: unit
- **GIVEN** selection and range focus are active
- **WHEN** the user switches between sequence and recorded-duration geometry
- **THEN** selection and focus remain stable
- **AND** sequence-only records do not acquire measured widths or timing values

#### Scenario: Search without removing context
Test: unit
- **WHEN** a search matches a subset of records
- **THEN** matching records remain discoverable
- **AND** nonmatching records are de-emphasized but retain their order and run boundaries

#### Scenario: Calls fold preserves model content
Test: unit
- **GIVEN** reasoning, a tool, text and another tool are interleaved in one Step
- **WHEN** Calls is folded
- **THEN** only tools are collapsed
- **AND** reasoning and text keep their chronological positions
- **AND** Turn state and manual fold overrides remain independent

#### Scenario: End-only record beside measured and legacy history
Test: unit
- **GIVEN** an end-only result, a measured interval and another sequence-only run
- **WHEN** Recorded geometry is selected
- **THEN** the result is positioned as an observed instant and the measured interval keeps its time geometry
- **AND** the legacy portion is visibly sequence-only without a fabricated time scale

#### Scenario: Measured execution differs from observed interval
Test: unit
- **WHEN** runtime execution duration differs from elapsed host observation
- **THEN** the inspector identifies the two measurements by source
- **AND** missing execution duration is not replaced by the host interval under the execution label

#### Scenario: Invalid local timing
Test: unit
- **WHEN** a timestamp regresses or an interval is invalid or exceeds supported bounds
- **THEN** only the affected timing is marked unavailable or invalid
- **AND** other measured intervals and event selection remain usable without fabricated negative durations

### Requirement: Hierarchical virtualized ledger

The ledger MUST organize records as run, turn, step, and event in chronological order using observed boundaries and explicit unknown groups. Large trajectories MUST use stable semantic identities and preserve scroll anchoring during historical prepend, live append, folding, search, and selection. Selecting a tool whose related events are outside the loaded window SHALL resolve its operation without requiring all Chat history to be loaded. Selecting a hidden child SHALL make the required ancestor path visible without resetting unrelated fold preferences.

#### Scenario: Timeline selection targets an offscreen row
Test: unit
- **GIVEN** a selected timeline span corresponds to a ledger row outside the viewport
- **WHEN** the selection changes
- **THEN** the matching ledger row becomes selected and visible
- **AND** the inspector shows that same record

#### Scenario: Append away from live edge
Test: unit
- **GIVEN** the user has scrolled away from the live edge
- **WHEN** new records arrive
- **THEN** they continue to be captured and added
- **AND** the viewport remains anchored instead of jumping to the end
- **AND** an explicit action can restore live following

#### Scenario: Prepend older history
Test: unit
- **GIVEN** a record is anchored in the viewport
- **WHEN** older records are prepended
- **THEN** the same semantic record and visual offset remain stable

#### Scenario: Inspect an operation spanning history pages
Test: integration
- **GIVEN** a call and its completed result are separated by thousands of events and different history pages
- **WHEN** the call is selected
- **THEN** its related result and final state become inspectable without loading the entire Chat
- **AND** not-yet-loaded related data is identified as loading or incomplete rather than no-result

#### Scenario: Select inside a folded group
Test: unit
- **WHEN** timeline selection targets a child hidden by folding
- **THEN** the required ancestor path becomes visible
- **AND** unrelated fold overrides, event identity and chronological context are preserved

### Requirement: Internal synchronized inspector

Selecting a timeline span or ledger row MUST synchronize timeline selection, ledger selection, and inspector content. The inspector MUST remain inside Trajectory rather than using the global Details sidebar, and MUST expose Summary, Payload, Result, Schema, and Timing views when corresponding data exists for the selected event or its correlated operation. The selected event identity and chronology MUST remain visible while related operation fields are resolved from their actual source events. Live completion SHALL update the selected call inspector without requiring reselection.

#### Scenario: Inspect a tool result
Test: unit
- **WHEN** the user selects a tool result
- **THEN** timeline and ledger identify the same record
- **AND** Summary identifies available run, turn, step, hierarchy, status, and error state
- **AND** applicable Payload, Result, Schema, and Timing views are available inside Trajectory

#### Scenario: Narrow surface
Test: unit
- **WHEN** the Trajectory surface is too narrow for a split ledger and inspector
- **THEN** the selected record opens through an internal detail state with a return path
- **AND** the global Details sidebar remains unchanged

#### Scenario: Selected call receives a successful result
Test: integration
- **GIVEN** a start event is selected while its operation is Running
- **WHEN** a successful matching result is captured
- **THEN** the inspector shows Completed and the result from the matching source event
- **AND** selection remains on the original start event
- **AND** Payload and Result resolve their respective source fields

#### Scenario: Select either side after reopening
Test: integration
- **WHEN** a completed operation is reopened and either its call or result is selected
- **THEN** both selections expose the same operation outcome and available input/output
- **AND** each selection preserves its own event identity and chronological position

### Requirement: Safe-by-default raw reveal

Payload and Result MUST show sanitized representations by default. The user MAY explicitly reveal one raw local field only on the device that captured the event. Raw reveal MUST be temporary presentation state and MUST NOT change synchronized state, export, or the stored sanitized representation. Consent to capture complete source MUST NOT automatically reveal it. Reveal SHALL validate local ownership and the exact persisted source reference and field. Legacy normalized source and complete original source SHALL be identified distinctly, with explicit fidelity and availability. Late responses MUST NOT restore data after selection, view, profile, Chat or source invalidation.

#### Scenario: Reveal and clear a sensitive value
Test: integration
- **GIVEN** a local tool result contains a sensitive value
- **WHEN** the inspector first opens
- **THEN** the value is sanitized
- **WHEN** the user explicitly reveals that field
- **THEN** the raw value appears only in the current Trajectory presentation
- **AND** changing the selected record, closing the surface, changing profile, or deleting the Chat clears it

#### Scenario: Raw source is not local or no longer available
Test: integration
- **GIVEN** the event was captured on another device or its local raw source cannot be resolved safely
- **WHEN** the user requests Reveal
- **THEN** the field is reported as unavailable
- **AND** synchronized transcript content is not substituted

#### Scenario: Capture consent does not reveal
Test: integration
- **WHEN** complete diagnostic capture is enabled and a sensitive tool event arrives
- **THEN** the inspector still shows a sanitized preview until explicit Reveal
- **AND** the watch stream contains no complete source body

#### Scenario: Foreign or forged source reference
Test: integration
- **WHEN** a Reveal request names another Chat, profile, device, field or unattached source reference
- **THEN** access is rejected without exposing the source body
- **AND** omitting an explicit target device does not bypass local-only authorization

#### Scenario: Source or selection invalidated during reveal
Test: integration
- **WHEN** source deletion, Chat deletion, profile change or selection change occurs while Reveal is pending
- **THEN** the pending response cannot restore the invalidated content

#### Scenario: Source fidelity and retryable error
Test: integration
- **WHEN** a source is normalized legacy, truncated, expired, corrupt or temporarily unreadable
- **THEN** the inspector distinguishes the source fidelity or unavailability reason
- **AND** no empty successful value is substituted
- **AND** a transient failure permits an explicit retry without crossing selection or ownership boundaries

### Requirement: Missing data remains unavailable

Any missing timestamp, duration, usage value, result, schema, raw source, or other optional field MUST render as unavailable, not captured, incomplete, loading or unsettled according to the actual source and execution state, rather than empty, zero, estimated, or successful. Unsettled SHALL mean an operation ended without a known result, not that an optional metric was never captured. A measured zero MUST remain distinguishable from unavailable usage. Completion of a run MUST NOT manufacture completion of a tool without a result.

#### Scenario: Optional data is absent
Test: unit
- **GIVEN** a record lacks one or more optional technical fields
- **WHEN** Summary or another inspector view renders
- **THEN** each absent field is represented as unavailable or unsettled according to its state
- **AND** the UI does not fabricate a value

#### Scenario: Optional metric cannot arrive
Test: unit
- **WHEN** an operation is Running but its source does not provide a metric or schema
- **THEN** that field is unavailable or not captured rather than promising an unsettled measurement

#### Scenario: Ended run lacks a tool result
Test: unit
- **WHEN** a run ends and an operation has no authoritative result after its history is resolved
- **THEN** the operation is unsettled rather than successfully completed
- **AND** a later authoritative result can settle it without inventing an earlier completion time

## ADDED Requirements

### Requirement: Correlated tool operation outcome

A tool operation SHALL correlate its call, updates and results within the executing profile, Chat, run and complete parent scope using its call identity. The outcome SHALL come from authoritative results, with failed results visibly distinguished. Replay and updates SHALL NOT reopen completed operations, reset their start time or create duplicate operations. Identity collisions SHALL be explicit instead of silently associating unrelated results. Operation resolution SHALL be bounded and local-authorized even when events span multiple history pages.

#### Scenario: Audited separated call and result
Test: integration
- **GIVEN** start event 174 and successful result 177 share one scoped call identity
- **WHEN** the start is inspected after the run ends
- **THEN** the operation is Completed with input from 174 and output from 177
- **AND** the selected event remains 174

#### Scenario: Parallel calls finish in reverse order
Test: unit
- **WHEN** concurrent calls finish in reverse order of their starts
- **THEN** each operation retains its own arguments, result and outcome

#### Scenario: Reused identity outside the operation scope
Test: unit
- **WHEN** the same call ID occurs in another Chat, run or parent scope
- **THEN** its results do not attach to the selected operation

#### Scenario: Repeated tool metadata after completion
Test: unit
- **WHEN** a completed tool receives an authoritative metadata update or replayed start
- **THEN** the operation retains its outcome and original start
- **AND** duplicate events do not create another execution

#### Scenario: Result arrives without a known start
Test: unit
- **WHEN** an authoritative result is observed without its start
- **THEN** its success or error is inspectable with input and start timing explicitly unknown
- **AND** no execution duration is fabricated

#### Scenario: Ambiguous identity collision
Test: unit
- **WHEN** multiple incompatible executions cannot be distinguished under the same scoped identity
- **THEN** the ambiguity is reported
- **AND** the inspector does not silently choose an arbitrary input/result pair

### Requirement: Usage and context provenance

Trajectory SHALL expose measured model consumption separately from observed context occupancy and context-window capacity. Each metric SHALL indicate its source and completeness. Authoritative final-message usage SHALL be counted once per scoped message across duplicate delivery, retry and resume. Cache and total values SHALL follow the source's definitions without double counting. Tool operations SHALL NOT inherit unrelated run or model token totals. Missing updates SHALL NOT erase last-known Chat context.

#### Scenario: Context known and consumption unknown
Test: integration
- **WHEN** the source reports context occupancy 109686 and capacity 272000 without measured consumption
- **THEN** those context values are displayed and consumption is unavailable
- **AND** no zero-token consumption claim is manufactured

#### Scenario: Reported zero remains measured zero
Test: unit
- **WHEN** an authoritative usage source explicitly reports zero
- **THEN** the value remains a measured zero distinct from missing usage

#### Scenario: Duplicate final-message usage
Test: integration
- **WHEN** the same final-message usage is delivered through multiple lifecycle events or replay
- **THEN** its contribution is counted once
- **AND** cumulative session values are not added again as new run deltas

#### Scenario: Cache and subordinate usage
Test: unit
- **WHEN** final usage includes cached tokens and subordinate-model contributions
- **THEN** aggregation does not count cache or the same subordinate usage twice
- **AND** incomplete components are identified as partial rather than a complete total

#### Scenario: Context changes after compaction
Test: integration
- **WHEN** a later compaction or model switch changes occupancy or capacity
- **THEN** the new observation does not rewrite the historical snapshot
- **AND** a missing context update does not clear the last-known Chat indicator

### Requirement: Runtime schema snapshots

Schema inspection SHALL use observed runtime tool definitions with provenance and observation time. Sanitized schema display SHALL retain the available structural contract without exposing sensitive examples, descriptions or values. Historical operations SHALL reference the snapshot observed for their execution context, with snapshot precision explicitly identified. Missing source SHALL be unavailable rather than a schema inferred from arguments, a normalized type description, or the current catalog mislabeled as historical. Full schema source SHALL obey complete-capture consent and explicit Reveal.

#### Scenario: Observed structural schema
Test: integration
- **WHEN** the runtime provides properties, required fields and enumeration constraints
- **THEN** the inspector exposes those available structural constraints with source provenance
- **AND** it does not substitute a static summary such as command-colon-string

#### Scenario: Catalog changes between observations
Test: integration
- **WHEN** a tool definition changes after an operation's associated snapshot
- **THEN** the old operation retains the prior snapshot and its observation precision
- **AND** the new catalog is not retroactively labeled as that operation's effective schema

#### Scenario: Runtime lacks schema capability
Test: integration
- **WHEN** the runtime provides no usable schema snapshot
- **THEN** Schema reports the unavailable source
- **AND** normalized type metadata is not presented as equivalent runtime-schema fidelity

#### Scenario: Schema contains sensitive examples
Test: integration
- **WHEN** an observed schema includes a sensitive example or description
- **THEN** default display and watch remain sanitized
- **AND** complete source is only retained under diagnostic consent and shown after authorized Reveal

### Requirement: Explicit next-run complete capture

Complete diagnostic capture SHALL require explicit consent for the next run of one local Chat in the current profile. The UI SHALL disclose active scope and retention/size limits before arming. Consent SHALL be consumed once at matching run start, SHALL NOT transfer to other Chats/profiles/runs and SHALL NOT rearm after restart. Opening or closing Trajectory SHALL NOT arm or stop capture. Explicit disarm/revocation SHALL stop new complete capture, including work queued after revocation, without interrupting execution. Disarming SHALL NOT implicitly delete previously retained source.

#### Scenario: Capture defaults off while semantic capture continues
Test: integration
- **WHEN** a run starts without diagnostic consent
- **THEN** semantic sanitized capture continues
- **AND** no new complete diagnostic source is retained

#### Scenario: Armed consent is specific and consumed once
Test: integration
- **GIVEN** next-run capture is armed for Chat A in profile X
- **WHEN** Chat B runs before Chat A
- **THEN** Chat B does not consume or inherit the consent
- **WHEN** Chat A starts its next run
- **THEN** capture applies only to that run
- **AND** a subsequent run requires new explicit consent

#### Scenario: View lifecycle is not consent lifecycle
Test: integration
- **WHEN** Trajectory is closed during an opted-in run
- **THEN** that run's capture continues independently of presentation
- **AND** reopening the view does not authorize another run

#### Scenario: Restart or profile change while armed
Test: integration
- **WHEN** the application restarts or changes profile with capture armed
- **THEN** the consent is cleared and no run is automatically rearmed

#### Scenario: Revoke an active capture
Test: integration
- **WHEN** the user revokes capture during a run
- **THEN** new complete writes are prevented without interrupting the run
- **AND** already retained source remains governed by its retention and explicit deletion policy

### Requirement: Isolated bounded diagnostic source

Complete diagnostic source SHALL remain isolated by executing device and profile, separate from the sanitized read model and recovery journal, with owner-only local filesystem access. Complete bodies SHALL NOT enter public event broadcasts, synchronized state, semantic watch streams, Chat Transcript Export, Live Voice context or logs. Local source reads and capture controls SHALL enforce Chat/profile ownership and SHALL NOT be forwarded to another device. Retention SHALL expire source seven days after capture, enforce a total 128 MiB retained-source budget per profile with oldest-first eviction, and limit each captured field to 1 MiB. Truncation or non-retention SHALL be explicit and SHALL NOT be represented as complete source. Source storage failures SHALL fail open for agent execution and expose the affected diagnostic gap.

#### Scenario: Sensitive source remains inside its authorized boundary
Test: integration
- **WHEN** opted-in arguments contain a sensitive value excluded from the normalized event
- **THEN** the value appears only in the authorized complete source and explicit local Reveal response
- **AND** it is absent from sanitized storage/watch, public broadcasts, sync/export, Voice, logs and recovery journal additions

#### Scenario: Source exceeds a field or profile limit
Test: integration
- **WHEN** a field exceeds 1 MiB or retained source reaches 128 MiB
- **THEN** bounded capture or oldest-first eviction respects those limits
- **AND** original size, truncation or unavailability is visible instead of a claim of complete source
- **AND** semantic history remains intact

#### Scenario: Expired diagnostic source
Test: integration
- **WHEN** retained source reaches seven days of age
- **THEN** the source is expired and cannot be newly revealed
- **AND** semantic records remain inspectable with source expiry identified

#### Scenario: Explicit diagnostic deletion and Chat deletion
Test: integration
- **WHEN** the user deletes diagnostics for a Chat
- **THEN** retained complete source and pending/revealed source state are invalidated without deleting semantic history or recovery journals
- **WHEN** the Chat is deleted locally, by synchronization or by Space cascade
- **THEN** its complete source is removed with its Trajectory lifecycle

#### Scenario: Archive retains semantic history
Test: integration
- **WHEN** a Chat is archived
- **THEN** its semantic Trajectory is retained
- **AND** its complete source remains subject to the same expiry and budget policy

#### Scenario: Capture source fails during execution
Test: integration
- **WHEN** the diagnostic queue saturates, writer fails, disk rejects writes or source becomes corrupt
- **THEN** the run and its normalized journal/transcript continue unaffected
- **AND** the diagnostic gap is visible without an unbounded queue or fabricated source

#### Scenario: Path or forwarding attack
Test: integration
- **WHEN** a request attempts cross-profile access, a forged source path or remote forwarding of diagnostic controls or source reads
- **THEN** the operation is rejected without leaking source data

### Requirement: Original received tool source fidelity

Under active diagnostic consent, capture SHALL preserve the arguments and result representation received from the runtime at tool execution boundaries before application normalization discards fields, subject to declared source limits. Inspection SHALL distinguish original received data, normalized legacy data, sanitized preview, truncation and not-captured data. The product MUST NOT claim to recover bytes already removed by the runtime or automatically read arbitrary external artifact paths. Capturing explicit argument fields SHALL NOT authorize collecting the process's inherited environment or credential files.

#### Scenario: Full bash arguments received
Test: integration
- **GIVEN** the runtime execution event supplies command, cwd, env and timeout arguments
- **WHEN** the opted-in call is captured and its original payload is explicitly revealed
- **THEN** all received fields within source limits are available rather than only normalized command text
- **AND** no inherited environment fields are collected unless they were explicit received arguments

#### Scenario: Structured result contains several representations
Test: integration
- **WHEN** an opted-in result contains text plus details, diff, image metadata or an artifact reference
- **THEN** the original received representation is preserved within source limits
- **AND** choosing a normalized diff preview does not erase the other original fields
- **AND** unavailable binary content or external references are identified without arbitrary path reads

#### Scenario: Runtime already summarized the output
Test: integration
- **WHEN** the runtime delivers an already-truncated result or external artifact reference
- **THEN** the inspector identifies that representation as the original received source with its limitations
- **AND** it does not claim the underlying unlimited stdout or artifact bytes were captured
