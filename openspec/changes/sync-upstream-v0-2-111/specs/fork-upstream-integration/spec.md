## ADDED Requirements

### Requirement: Preserve fork capabilities when refreshing upstream
The integrated application SHALL retain the fork's Workers, orchestrator grants, direct Worker notifications and Details widgets while exposing the accepted upstream search, PR and navigation capabilities.

#### Scenario: Worked project totals survive the upstream refresh
- **Test:** unit
- **WHEN** an expanded Projects worked section observes two local checkouts with different diffs
- **THEN** each row shows only its exact checkout's totals and hiding Details releases the subscription

#### Scenario: New wire methods use the existing registry
- **Test:** unit
- **WHEN** a client uses workspace search or PR inspection on the owning device
- **THEN** the registered method and engine handler agree on request, response, forwarding and authorization

#### Scenario: Responsive composer follows the rendered panel geometry
- **Test:** unit
- **WHEN** a docked chat opens or closes the Right, Files, or Sidebar column
- **THEN** each column starts continuously and advances monotonically with less than 10% progress after 16 ms; the composer tracks the live conversation width; the right-titlebar strip stays aligned to the rendered right-pane bounds as neighboring columns move; and right-pane takeover retains its covered layout width

#### Scenario: The requested macOS release is installed without launching
- **Test:** none — packaging has manual native verification per scripts/AGENTS.md
- **WHEN** the release packaging script completes locally
- **THEN** the new executable and bundle replace the installed Zeron app and the script does not open it
