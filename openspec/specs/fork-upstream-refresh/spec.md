# Fork Upstream Refresh Specification

## Purpose

Keep accepted upstream Chat, workspace and desktop behaviors available while preserving the fork's security, managed Usage and panel ownership contracts.

## Requirements

### Requirement: Projectless Chats do not index the user's home

The application SHALL assign new projectless Chats their own host-owned scratch folder when the host supports the capability and SHALL exclude home roots from workspace search.

#### Scenario: Projectless execution resolves on its owning device
- **Test:** integration
- **WHEN** a projectless Chat runs on a scratch-capable host
- **THEN** its cwd is its own scratch folder, not the user's home, and Files/search resolve the same host-owned target

### Requirement: Device worktree preferences preserve fork ownership

The application SHALL support device-local worktree location preferences while preserving common Chat/Workers ownership, setup and deletion protections.

#### Scenario: Custom location is selected without bypassing setup
- **Test:** integration
- **WHEN** a Chat creates a worktree with a device location preference
- **THEN** creation uses that location and still follows the shared worktree lifecycle and reports setup errors without abandoning ownership

#### Scenario: Ancestor location does not falsely imply a nested checkout
- **Test:** unit
- **WHEN** the chosen location contains the repository but the common hashed destination is outside that repository
- **THEN** creation is allowed, while a destination inside the repository is refused before any checkout or branch is created

#### Scenario: Changing the destination does not skip historical preparation
- **Test:** integration
- **WHEN** setup failed for an owned checkout and the device's location preference changes before retry
- **THEN** retry prepares that same registered checkout through the ownership journal rather than treating it as external

### Requirement: Accepted desktop features coexist with fork surfaces

The application SHALL expose PR side panes, adaptive Session information, file multiselect, overscroll and idle harness icons without removing Details/Workers/Usage, the right-side terminal or Enter/Steer.

#### Scenario: Session metadata follows effective Details visibility
- **Test:** unit
- **WHEN** Details is visible or compressed to zero width
- **THEN** Session information omits duplicate location metadata only while Details has effective visible width and retains its actions

#### Scenario: Active Chat input remains steering
- **Test:** unit
- **WHEN** Enter submits nonempty input during an active run
- **THEN** the durable steer path retains attachments and the upstream queue does not replace it

### Requirement: Browser profiles remain isolated across identities

Embedded browser state SHALL persist within the owning identity's profile and SHALL NOT share stored website data across identities.

#### Scenario: Profile switch does not reuse another identity's storage
- **Test:** unit
- **WHEN** the identity changes or a browser tab is restored
- **THEN** profile selection restores only that identity's tabs/store and closing a tab preserves its profile's persistent state

### Requirement: Managed Usage reset information stays on its owning surface

Managed Provider Usage SHALL remain device-local in Details and Accounts and SHALL expose available reset time information without restoring the removed upstream usage ring.

#### Scenario: Reset tooltip preserves managed provider rows
- **Test:** unit
- **WHEN** a managed provider reports quota windows and reset timestamps
- **THEN** its existing Usage surface presents reset information while preserving provider ordering and local usage lines
