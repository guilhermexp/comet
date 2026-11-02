## MODIFIED Requirements

### Requirement: Server links open in the internal browser
Opening a server from Details Servers SHALL use the Chat link router and the internal browser, reaching a remote host's server through its preview route. Ordinary Chat links SHALL keep preferring the internal browser; explicit external choices, pull request policy, special schemes and OAuth keep their handling. Failures SHALL NOT silently open the external browser. A server that resolves later through discovery SHALL open without taking keyboard focus or replacing an address the user is editing.

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

#### Scenario: Late discovery keeps the editing focus
Test: unit — GPUI Shell routing fixture.
- **WHEN** a remote server is waiting for discovery and the user is typing in the composer or the Browser address field when discovery lists it
- **THEN** the server opens once in the Browser
- **AND** the same input keeps keyboard focus and further typing continues there with the earlier text intact
