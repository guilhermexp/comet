# Workers Project Picker Specification

## Purpose
Provide the same folder browsing experience for local Worker projects and Orchestrator projects while preserving their ownership boundaries.

## Requirements
### Requirement: Shared project palette
Worker project addition SHALL use the same in-app folder palette as Orchestrator, retaining folder search, keyboard navigation, hidden folders and locations. Confirmation SHALL register the current folder as a Worker project without creating an Orchestrator Space or launching a Worker. Cancellation SHALL leave registered projects unchanged.

#### Scenario: Open and cancel Worker project selection
Test: none — native GPUI QA; no automated render harness.
- **WHEN** the user opens New project or Command-K in Workers
- **THEN** the shared folder palette opens and Escape dismisses without registration

#### Scenario: Confirm local folder
Test: none — native GPUI QA; registration queue already covered independently.
- **WHEN** the user confirms the browsed folder in Worker mode
- **THEN** that folder becomes the selected Worker project without creating a Space

### Requirement: Device scope follows destination
Worker project browsing and confirmation SHALL permit only the known local device. Orchestrator SHALL retain its existing local and remote device choices. A missing local identity SHALL NOT fall back to a remote device for Workers.

#### Scenario: Worker device choices
Test: unit — shell project palette device eligibility.
- **WHEN** local and remote devices are registered
- **THEN** Worker mode only permits the local device

#### Scenario: Local identity unavailable
Test: unit — shell project palette device eligibility.
- **WHEN** local identity is unavailable
- **THEN** Worker mode does not permit any remote device

#### Scenario: Orchestrator device choices
Test: unit — shell project palette device eligibility.
- **WHEN** the Orchestrator palette is opened
- **THEN** local and remote devices remain available
