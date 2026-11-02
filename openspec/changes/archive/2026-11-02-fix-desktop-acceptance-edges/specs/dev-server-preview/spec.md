## MODIFIED Requirements

### Requirement: Activating Browser closes Details
Activating the Browser right panel SHALL close Details through the existing close and persistence sequence, whether the Browser is created, a new tab is added, a navigation opens it or an existing Browser tab is selected. File previews (PDF, image, CSV and other files) SHALL NOT close Details. The user SHALL remain able to reopen Details manually while Browser is selected, and Details SHALL NOT be presented behind the voice stage.

#### Scenario: Opening Browser with Details open
Test: unit — GPUI Shell panel fixture.
- **WHEN** Details is open and the user opens Browser by any entry path
- **THEN** Details closes and its closed state and width are saved through the existing sequence

#### Scenario: File preview keeps Details
Test: unit — GPUI Shell panel fixture.
- **WHEN** Details is open and the user opens a PDF, image or CSV preview
- **THEN** Details remains open

#### Scenario: Details already closed
Test: unit — GPUI Shell panel fixture.
- **WHEN** Details is already closed and Browser is activated
- **THEN** no additional close transition or preference write occurs

#### Scenario: First Browser and reopening the pane onto a Browser
Test: unit — GPUI Shell panel fixture.
- **WHEN** Details is open and the first Browser opens with no prior surface, or the hidden right pane is reopened onto a Browser
- **THEN** Details closes
- **AND WHEN** the user then reopens Details while that same Browser stays visible
- **THEN** Details stays open across repaints

#### Scenario: Returning to a Chat restores its Browser
Test: unit — GPUI Shell panel fixture.
- **WHEN** Chat A shows a Browser, the user switches to Chat B (no Browser), opens Details there and returns to A
- **THEN** A's restored Browser closes Details once
- **AND** switching to a destination without a Browser stops tracking the earlier Browser
- **AND WHEN** the user reopens Details while that same Browser stays visible
- **THEN** Details stays open across later updates and repaints
