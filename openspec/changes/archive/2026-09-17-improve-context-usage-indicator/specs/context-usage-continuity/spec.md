## MODIFIED Requirements
### Requirement: Retain the last context measurement between turns
The engine SHALL preserve the last context usage snapshot for each Chat while a new turn waits for a newer runtime measurement, merging independently reported tokens and capacity without treating missing values as zero.

#### Scenario: A new process starts for a measured chat
Test: engine unit regression over turn-start state transition.
- **WHEN** a measured Chat starts another turn
- **THEN** its previous measurement remains until newer runtime data arrives

#### Scenario: A chat has never reported usage
Test: composer unit.
- **WHEN** no measurement is available
- **THEN** the indicator displays an unknown percentage and explains that usage has not been reported

#### Scenario: Partial and zero measurements
Test: proto, doc and engine unit.
- **WHEN** the runtime reports only capacity, only tokens, or zero tokens
- **THEN** missing fields retain previous measurements and real zero replaces prior token usage
- **AND** partial state survives persistence and older complete snapshots remain readable

## ADDED Requirements
### Requirement: Present current reported context clearly
The composer SHALL show a percentage beside the circle and an observing tooltip with exact used/capacity/remaining tokens, explicit missing data, and percentages above 100 when reported.

#### Scenario: Usage changes while tooltip is open
Test: UI state unit and native manual render (none tier).
- **WHEN** the selected Chat receives newer usage
- **THEN** both circle and open tooltip reflect it without a new turn or reopening

#### Scenario: OMP compaction finishes
Test: harness integration and composer/shell unit.
- **WHEN** OMP confirms background compaction completion
- **THEN** the fresh usage is published before Done and the compact action and transcript marker remain available

### Requirement: Normalize runtime context independently of billing
Harnesses SHALL use reported context facts, excluding child Claude usage and aggregate result totals from primary prompt occupancy.

#### Scenario: Claude reports model-specific usage
Test: harness unit and integration fixtures.
- **WHEN** a primary assistant message reports cached input and the result reports modelUsage capacity
- **THEN** occupancy counts primary input plus cached input and capacity matches that model without a hardcoded fallback

#### Scenario: Codex reports partial or zero usage
Test: harness unit.
- **WHEN** Codex reports either zero tokens or only one context field
- **THEN** the measurement is emitted rather than discarded
