# dev-server-preview Specification

## Purpose

Expose running development servers through native browser tabs with stable URLs and authenticated cross-device access.

## Requirements

### Requirement: Local discovery and native browsing
The app SHALL discover running project development servers and open their stable preview URLs in native browser tabs with navigation, resize and live reload support.

#### Scenario: Open a development server
Test: unit + integration — discovery and proxy; none — native BCU browser.
- **WHEN** a project starts an HTTP development server
- **THEN** the preview becomes discoverable and opens in the browser with working navigation and HMR

### Requirement: Peer preview access
A signed-in peer SHALL access an authorized device preview through the preview pairing transport. Pairing failures, unavailable hosts and unsupported peers SHALL be visible; a failed connection MUST NOT imply successful page loading.

#### Scenario: Open preview hosted by another device
Test: integration — pairing, authorization and transport; none — live peer validation.
- **WHEN** an authorized peer selects a preview belonging to an online host
- **THEN** the native browser connects to that preview and relays HTTP and live-update traffic within the authenticated pairing

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

### Requirement: Remote server selection reports host preview eligibility
Dev-server rows SHALL carry optional eligibility from host attribution and the selected Chat's shared watch visibility predicate. Missing Chat context or old-host metadata SHALL remain unknown. The polling key SHALL include Chat identity. Local routing and authorized preview URLs SHALL remain unchanged, without expanded exposure authority.
#### Scenario: Unowned remote development listener
Test: unit — host candidate/list fixtures and GPUI Browser consumer; none — safe native component smoke.
- **WHEN** a command-matching remote listener is ineligible and finished snapshots never advertise it
- **THEN** selecting it displays the unavailable port and performs no local or external navigation
#### Scenario: Eligible delayed or already-published service
Test: unit — Browser snapshot consumer.
- **WHEN** eligible discovery arrives later or an authorized preview is already published
- **THEN** selection uses its preview route and respects manual-navigation supersession

#### Scenario: Worked root belongs to a different Chat
Test: unit — host listener attribution and shared selected-Chat predicate.
- **WHEN** Chat A in /a mentions a listener owned by Chat B in worked root /b
- **THEN** the row remains listed but is unavailable to A's preview watch
- **AND** A's own listener started elsewhere and another hosted Chat in A's exact cwd remain eligible
#### Scenario: Same roots switch selected Chat
Test: unit — Details polling lifecycle with detached models.
- **WHEN** selected Chat changes with identical device and roots
- **THEN** previous rows and eligibility are invalidated before the new poll

### Requirement: Server selection survives the first subscription delivery
A remote port selection SHALL survive an outstanding first watch snapshot. Published authorized URLs SHALL take precedence over false or unknown eligibility, including later advertisements. An absent false/unknown selection SHALL settle to an explicit unavailable reason after delivery, or immediately without a watch. Manual navigation and watch rebinding SHALL cancel the selection.
#### Scenario: First snapshot arrives after selection
Test: unit — asynchronous GPUI snapshot consumer; none — native component unavailable smoke.
- **WHEN** a fresh Browser selects a port before its first snapshot
- **THEN** a published matching URL opens despite false or unknown metadata
- **AND** an absent matching port settles to unavailable without indefinite loading
#### Scenario: Navigation supersedes the selection
Test: unit — asynchronous Browser consumer and existing navigation regression.
- **WHEN** the user navigates manually before the first snapshot arrives
- **THEN** late preview delivery does not replace that page
