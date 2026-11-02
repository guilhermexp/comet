## ADDED Requirements
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
