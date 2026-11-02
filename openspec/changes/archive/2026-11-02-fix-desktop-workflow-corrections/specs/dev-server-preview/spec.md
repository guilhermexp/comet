## ADDED Requirements

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

### Requirement: Server links open in the internal browser
Opening a server from Details Servers SHALL use the Chat link router and the internal browser, reaching a remote host's server through its preview route. Ordinary Chat links SHALL keep preferring the internal browser; explicit external choices, pull request policy, special schemes and OAuth keep their handling. Failures SHALL NOT silently open the external browser.

#### Scenario: Local server link
Test: unit — GPUI Shell routing fixture.
- **WHEN** the user opens a local server row in Details Servers
- **THEN** an internal Browser surface opens on that server for the selected Chat
- **AND** no external browser is launched

#### Scenario: Stale Chat context
Test: unit — GPUI Shell routing fixture.
- **WHEN** the server event refers to a Chat that is no longer selected
- **THEN** nothing opens and no external browser is launched

#### Scenario: Explicit external preference
Test: unit — GPUI Shell routing fixture.
- **WHEN** the user's preference opens web links externally
- **THEN** the server link follows that explicit preference

#### Scenario: Links pasted in the user's own message
Test: unit — GPUI Shell routing fixture.
- **WHEN** the user clicks a GitHub or YouTube link chip in their own sent message with the default preference
- **THEN** it opens in the session Browser and no external browser is launched
- **AND WHEN** the user has explicitly chosen to open web links outside Zeron
- **THEN** the same chip opens externally

#### Scenario: A manual navigation supersedes a pending server
Test: unit — GPUI Shell routing fixture.
- **WHEN** a remote server is waiting for discovery and the user navigates the Browser to another address
- **THEN** a later preview snapshot listing that server does not replace the user's page
- **AND WHEN** no newer navigation happened
- **THEN** the pending server still opens once discovery lists it
