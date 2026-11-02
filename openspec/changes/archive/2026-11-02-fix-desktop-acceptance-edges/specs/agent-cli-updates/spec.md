## MODIFIED Requirements

### Requirement: Pending OMP updates reconcile external installations while waiting
While an internal OMP update is waiting for active execution leases, the engine SHALL detect an externally installed version that is current, release the pending update, report the harness as current and cancel only the waiting installation. It SHALL NOT terminate, restart or reject active runtime work. Exclusive installation and background or accepted-steer drain semantics SHALL be preserved, and a race between lease acquisition and reconciliation SHALL leave exactly one terminal outcome.

#### Scenario: External update during WaitingForIdle
Test: unit — engine updater fixture with an isolated fake executable.
- **WHEN** an update is waiting for an active session and the executable on disk becomes the latest version
- **THEN** the pending update is released and the harness reports current
- **AND** the active session's execution lease is untouched

#### Scenario: Lease acquired before reconciliation
Test: unit — engine updater fixture.
- **WHEN** the lease is acquired and installation begins before the external change is observed
- **THEN** installation proceeds exclusively and the post-install recheck determines the result

#### Scenario: Cancel while the external version probe stalls
Test: unit — engine updater fixture with a stalled fake `--version`.
- **WHEN** the user cancels a waiting update while its external-change version probe is still running
- **THEN** the cancel returns at once, clearing the update intent, the pending marker and the queued lease
- **AND** only the diagnostic probe process is stopped; no installer runs and active work is untouched

#### Scenario: External update at a new path
Test: unit — engine updater fixture with two isolated installs.
- **WHEN** a waiting update's harness now resolves to a newly installed executable at another path, and the old path still reports the old version or no longer exists
- **THEN** the reconciliation probes the newly resolved executable, releases the pending update and reports the harness current
- **AND** no installer runs and the active runtime keeps its lease

#### Scenario: Retarget during a current probe
Test: unit — engine updater fixture with a held fake `--version` and two isolated installs.
- **WHEN** the probe of a binary that answers with the latest version is still running and the harness is retargeted to an older installation before it answers
- **THEN** the update keeps waiting and the harness is not reported current
- **AND** the older installation is not recorded as checked, so the watcher still detects it
- **AND** the active lease survives and no installer runs

#### Scenario: An older replacement is probed once
Test: unit — engine updater fixture counting `--version` calls.
- **WHEN** a waiting update's executable is replaced by a build that is still older than the release and reconciliation runs twice without any further change
- **THEN** only one additional version probe runs
- **AND** the update keeps waiting and no installer runs

#### Scenario: The installation is replaced while the update waits
Test: unit — engine updater fixture with two isolated fake installs.
- **WHEN** an update queued on installation A waits for an active run and the harness meanwhile resolves to installation B, still older than the release, which the waiting reconciliation probed and recorded
- **THEN** once the active run ends, the update runs B's own installation plan and verifies B
- **AND** A is left untouched, whether it still exists or has been removed
