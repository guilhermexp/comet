## ADDED Requirements
### Requirement: Existing package repair Retry retains trusted installation identity
A Failed/Install Retry MAY use an independent package updater after a failed probe only if retained attempt provenance proves unchanged launcher, owner, package/tag and candidate. Missing provenance or candidate invalidation SHALL refuse fallback. Failed probes SHALL NOT publish observations or satisfy Current. Cancellation, owner serialization, final identity validation and post-install verification SHALL remain required.
#### Scenario: Supporting dependency breaks during an update
Test: unit — engine coordinator with disposable npm-owned Codex scripts.
- **WHEN** an update fails after breaking a supporting version file but leaves the checked launcher unchanged
- **THEN** the existing Retry runs the same prefix-owned npm and verifies its repaired version
#### Scenario: Repair identity is no longer trusted
Test: unit — disposable coordinator fixtures.
- **WHEN** the executable identity or owning installation changed, or the candidate was invalidated
- **THEN** a failed diagnostic cannot authorize a repair mutation

#### Scenario: Owner alias changes before Retry begins
Test: unit — disposable failed npm attempt and Bun alias to the same canonical launcher.
- **WHEN** the owner alias changes before a failed-probe Retry enters its wait
- **THEN** neither a second npm nor Bun mutation runs using newly inferred authority
