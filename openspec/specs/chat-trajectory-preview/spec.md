# chat-trajectory-preview Specification

## Purpose
Provide a safe, device-local observability surface for every main Chat run, preserving execution chronology and technical detail without changing synchronized Chat, export, or recovery behavior.

## Requirements

### Requirement: Trajectory entry point and surface lifecycle

The application MUST show a Trajectory control beside Capture whenever a main Chat is selected. Activating it MUST open and select that Chat's Trajectory surface in the existing right pane, and the control MUST indicate when that surface is selected.

#### Scenario: Open and focus one surface
Test: integration
- **GIVEN** a selected Chat without an open Trajectory surface
- **WHEN** the user activates Trajectory twice
- **THEN** the first activation opens and selects one right-pane surface for that Chat
- **AND** the second activation focuses the same surface without creating a duplicate
- **AND** the titlebar control indicates that Trajectory is active

#### Scenario: No selected main Chat
Test: unit
- **WHEN** no main Chat is selected or the current mode is CLI Workers
- **THEN** the Trajectory control is not shown

### Requirement: Existing right-pane behavior

The Trajectory surface MUST participate in the existing right-pane tab, focus, close, reorder, resize, expand, and narrow-viewport takeover behavior. A Chat MUST have at most one open Trajectory surface, while different Chats MAY each retain an independent surface and presentation state.

#### Scenario: Switch between Chat surfaces
Test: integration
- **GIVEN** two Chats each have an open Trajectory surface with different selection and scroll state
- **WHEN** the user switches between those Chats
- **THEN** each Chat restores its own surface and presentation state
- **AND** records or state do not cross between Chats

#### Scenario: Delete or archive a Chat
Test: integration
- **WHEN** an open Chat is archived
- **THEN** its local Trajectory remains available
- **WHEN** that Chat is deleted
- **THEN** its Trajectory surface closes and its local Trajectory history is removed

### Requirement: Capture independent of presentation

The executing device MUST capture Trajectory records for every local main Chat run without requiring the preview to be open. Closing the surface MUST stop only presentation and MUST NOT stop or alter capture.

#### Scenario: Capture while closed
Test: integration
- **GIVEN** a local Chat run is executing
- **WHEN** the user closes Trajectory and events continue to occur
- **THEN** capture continues
- **AND** reopening Trajectory shows the records emitted while the surface was closed

#### Scenario: Capture with no subscriber
Test: integration
- **WHEN** a complete run occurs without any Trajectory surface or subscriber
- **THEN** its semantic records remain available after the run and after application restart

### Requirement: Device-local isolated history

Trajectory history MUST remain device-local and MUST NOT enter synchronized Chat state, Chat Transcript Export, or Run Journal recovery state. The Trajectory read model MUST remain a distinct product contract from both Chat Transcript and Run Journal.

#### Scenario: Inspect synchronized and exported state
Test: integration
- **GIVEN** a Trajectory record containing timing, usage, status, and a sanitized tool result
- **WHEN** Chat state is synchronized or exported
- **THEN** no Trajectory record, timing, usage, or raw reveal state is added by this capability

#### Scenario: Storage degradation
Test: integration
- **WHEN** Trajectory storage or migration fails during a run
- **THEN** the agent run, Run Journal recovery, and Chat Transcript continue
- **AND** Trajectory reports the affected interval as degraded instead of claiming complete history

### Requirement: Ordered multi-run event model

Each captured record MUST preserve stable ordering, observed event time when available, run identity, semantic kind, status, error state, and available turn, step, call, duration, usage, and correlation data. The preview MUST combine every run for the selected Chat captured on the current device and MUST render explicit run boundaries.

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

### Requirement: Honest legacy projection

Eligible local legacy journal history MUST be imported idempotently. Events without per-event timestamps MUST use sequence geometry, and Duration and Timing MUST be unavailable for those records. Unknown run boundaries MUST NOT be invented.

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

### Requirement: Coherent history and live updates

Opening historical data and following a live run MUST converge on one stable ordering and projection contract. Records delivered across the history-to-live boundary MUST appear exactly once, and missing live ranges MUST trigger an explicit degraded or resnapshot state rather than silent reordering.

#### Scenario: Event occurs while opening
Test: integration
- **WHEN** a new event is captured while a Trajectory surface establishes its historical view
- **THEN** the event appears exactly once in the resulting ordered view

#### Scenario: Reconnect after a watermark
Test: integration
- **GIVEN** a surface has already received records through a known watermark
- **WHEN** its live watch reconnects
- **THEN** only missing later records are added
- **AND** duplicate delivery does not create duplicate rows

### Requirement: Three-lane timeline semantics

The preview MUST render fixed Input, Model, and Tools overview lanes. System, user, and context records MUST map to Input; assistant and model records MUST map to Model; tool and subordinate-tool records MUST map to Tools. Errors MUST remain visibly distinguishable from successful records in both timeline and ledger.

#### Scenario: Classify and select records
Test: unit
- **GIVEN** one input record, one assistant record, one tool call, and one failed tool result
- **WHEN** the timeline renders
- **THEN** each record appears in its required lane in chronological order
- **AND** the failed result retains its semantic lane while receiving a distinct error state

### Requirement: Duration, Turns, Calls, and Search controls

The toolbar MUST provide Duration, Turns, Calls, and Search controls. Duration MUST switch between equal-width sequence geometry and recorded-duration geometry without presenting missing timing as measured data. Turns MUST fold turns independently from Calls folding tool calls under assistant steps. Search and range focus MUST de-emphasize nonmatching records without removing chronological context.

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

### Requirement: Hierarchical virtualized ledger

The ledger MUST organize records as run, turn, step, and event in chronological order. Large trajectories MUST use stable semantic identities and preserve scroll anchoring during historical prepend, live append, folding, search, and selection.

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

### Requirement: Internal synchronized inspector

Selecting a timeline span or ledger row MUST synchronize timeline selection, ledger selection, and inspector content. The inspector MUST remain inside Trajectory rather than using the global Details sidebar, and MUST expose Summary, Payload, Result, Schema, and Timing views when corresponding data exists.

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

### Requirement: Safe-by-default raw reveal

Payload and Result MUST show sanitized representations by default. The user MAY explicitly reveal one raw local field only on the device that captured the event. Raw reveal MUST be temporary presentation state and MUST NOT change synchronized state, export, or the stored sanitized representation.

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

### Requirement: Missing data remains unavailable

Any missing timestamp, duration, usage value, result, schema, raw source, or other optional field MUST render as unavailable or unsettled rather than empty, zero, estimated, or successful.

#### Scenario: Optional data is absent
Test: unit
- **GIVEN** a record lacks one or more optional technical fields
- **WHEN** Summary or another inspector view renders
- **THEN** each absent field is represented as unavailable or unsettled according to its state
- **AND** the UI does not fabricate a value
