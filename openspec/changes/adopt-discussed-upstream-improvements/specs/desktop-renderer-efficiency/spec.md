## Purpose

Reduce background rendering work while keeping the native desktop responsive to visible updates and system wake.

## ADDED Requirements

### Requirement: Presentation-only invalidation
The desktop SHALL retain fresh device/session timestamps without republishing unchanged visible presentation, while still notifying changes to status, errors, membership and displayed data.

#### Scenario: Heartbeat refresh
Test: unit — state presentation comparison.
- **WHEN** only non-visible heartbeat timestamps advance
- **THEN** the stored timestamps update and the UI receives no redundant presentation notification

### Requirement: Idle and wake rendering
Idle windows SHALL stop continuous display callbacks and SHALL resume rendering after an interaction, system wake or temporary display-link registration failure. Wrapping, transparency, blur, native previews and focus MUST remain usable.

#### Scenario: Resume after idle
Test: none — BCU native window and wake acceptance; unit for lifecycle decisions when exposed.
- **WHEN** a previously idle or suspended window receives an interaction or display update
- **THEN** the next visible state renders and input remains responsive
