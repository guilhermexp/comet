## ADDED Requirements

### Requirement: Staged pasted text cards render inside the composer
Staged pasted-text cards and other staged attachment cards SHALL render inside the same composer border and background, above the editable text, and the composer layout SHALL reserve their height. They SHALL NOT overlap the Todo or queue strips above the composer or the composer actions. Paste thresholds, full pasted content, removal, send behavior, normal attachments and keyboard focus SHALL be preserved.

#### Scenario: Pasted card with typed text and Todo
Test: unit — GPUI composer layout fixture; none — native appearance acceptance.
- **WHEN** a long paste becomes a Pasted Text card, the user types text and a Todo strip is visible
- **THEN** the card bounds are inside the composer surface and above the editor
- **AND** the card does not intersect the Todo strip or the composer actions
- **AND** sending delivers the full pasted content and the typed text

#### Scenario: Narrow and wide composers
Test: unit — GPUI composer layout fixture.
- **WHEN** the composer is narrow or wide, compact or expanded, empty or with a draft, with several cards and images
- **THEN** every card remains inside the composer surface and the composer grows to contain it
