## ADDED Requirements

### Requirement: Git identity groups project registrations across devices
The project picker and Chat sidebar SHALL group registrations by stable Git repository identity while preserving each device, checkout and subfolder execution target. Root commit identity SHALL survive remote renames; unavailable history SHALL fall back to normalized remote identity and then a device-local identity. Project filters SHALL include the selected repository's registrations across devices and retain saved selection compatibility.

#### Scenario: Clones and worktrees on multiple devices
Test: unit — engine repository identity and shared mobile/desktop grouping fixtures.
- **WHEN** two devices register clones or worktrees of the same repository
- **THEN** project grouping and filtering include both registrations
- **AND** new-Chat creation selects a concrete device and checkout
- **AND** distinct monorepo subfolders and unrelated repositories remain distinguishable
