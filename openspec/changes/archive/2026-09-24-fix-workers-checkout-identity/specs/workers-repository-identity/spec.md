## ADDED Requirements

### Requirement: Directory identity survives device renumbering
On macOS the app SHALL preserve a repository identity across changes to the operating system's temporary device number when the volume and Git common directory remain the same. Replacement of that directory or volume MUST remain detectable.

#### Scenario: The same directory is observed after a device number change
- Test: integration — `checkout_identity_recovery` validates stable identity observation and reconciliation
- **WHEN** the same volume and Git common directory are observed with a different device number
- **THEN** repository and checkout IDs remain unchanged and no new identity conflict is introduced

#### Scenario: A different directory occupies the registered path
- Test: integration — `checkout_identity_recovery` and existing `project_identity_edges`
- **WHEN** a different Git common directory or volume occupies a registered canonical path
- **THEN** the old association remains blocked and sessions are not reassigned

### Requirement: Obsolete fingerprints have explicit guarded recovery
The app SHALL offer diagnosis and explicit revalidation of the selected repository identity using fresh Git and filesystem evidence. Recovery MUST require the caller's expected old and observed fingerprints, reject concurrent changes, preserve unrelated state and IDs, and clear only the fingerprint conflict being repaired. Ambiguous legacy identities SHALL NOT be accepted automatically merely because their inodes match.

#### Scenario: The owner revalidates an obsolete fingerprint
- Test: integration — `checkout_identity_recovery` with a private state file and disposable Git repositories
- **WHEN** the caller confirms the expected old and current fingerprints for an available repository
- **THEN** the stable fingerprint is persisted and matching primary and linked checkouts can launch in their exact paths
- **AND** sessions, presets and unrelated fields remain unchanged

#### Scenario: State or filesystem changes after diagnosis
- Test: integration — `checkout_identity_recovery` exercises stale expectations and unrelated conflicts
- **WHEN** the stored identity or observed directory differs from the values supplied for recovery
- **THEN** recovery fails without altering state or clearing unrelated conflicts

### Requirement: Known blocked projects report their launch blocker
Every launch surface SHALL distinguish an unknown project from a registered checkout blocked by conflict, unavailable filesystem, archive, removal, or unreadable identity metadata. Failure MUST occur before spawning a worker and MUST retain the exact requested checkout.

#### Scenario: A listed project has an identity conflict
- Test: integration — `checkout_identity_recovery` through controller MCP and the real Host route
- **WHEN** launch is requested for a registered checkout with an identity conflict
- **THEN** the error identifies the conflict and recovery action rather than saying the project is unknown
- **AND** no worker process or session is created

#### Scenario: The requested project does not exist
- Test: integration — `checkout_identity_recovery`
- **WHEN** launch names an unregistered ID
- **THEN** the error identifies the unknown project without adopting another checkout
