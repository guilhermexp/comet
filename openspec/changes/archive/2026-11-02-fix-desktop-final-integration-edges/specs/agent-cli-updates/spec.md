## MODIFIED Requirements

### Requirement: Pending OMP updates reconcile external installations while waiting
While an internal OMP update is waiting for active execution leases, the engine SHALL detect an externally installed version that is current, release the pending update, report the harness as current and cancel only the waiting installation. It SHALL NOT terminate, restart or reject active runtime work. Exclusive installation and background or accepted-steer drain semantics SHALL be preserved, and a race between lease acquisition and reconciliation SHALL leave exactly one terminal outcome. A successful stable older-version observation SHALL publish its installed version with its accepted fingerprint. Every stable observation SHALL compare its package/release family with the original candidate before interpreting the captured target. A changed family SHALL atomically publish the observed installed version and invalidate the candidate and applicability, including across cancellation and future cleanup. Cleanup SHALL dispatch at most one existing full check after pending intent is cleared, unless shutdown, harness disablement or Off policy prevents it. After exclusive admission, planning SHALL use the observed executable and its version; if its release/package identity differs from the candidate’s identity, the updater SHALL release admission and pending intent and refresh through the existing full check before any installation.

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

#### Scenario: Older replacement survives cancellation
Test: unit — temporary executable with coordinator and registry.
- **WHEN** A 1.0 is replaced by B 1.5 while waiting for target 2.0 and the update is cancelled
- **THEN** the watch publishes installed 1.5 and unchanged B is not probed again

#### Scenario: Replacement changes release family
Test: unit — disposable global-like npm paths and scripts.
- **WHEN** checked OpenCode v2 is replaced by v1 before exclusive mutation
- **THEN** no stale candidate mutates either installation and the existing full check runs only after exclusive admission is released

#### Scenario: Cross-family observation survives cancellation and reapply
Test: unit — disposable OpenCode installations and real coordinator/registry.
- **WHEN** pending OpenCode v2 observes older v1, is cancelled and Apply is requested again
- **THEN** v1 remains displayed, the v2 candidate is inapplicable, neither installer runs and the existing full check obtains execution admission

#### Scenario: Cancellation or future cleanup after family observation
Test: unit — coordinator operation future and disposable version probes, without production hooks.
- **WHEN** cancellation or dropping the operation follows publication of a different release family, while waiting or under exclusive admission
- **THEN** candidate invalidation survives cleanup and at most one full check runs after pending is cleared
- **AND** shutdown, a disabled harness or Off policy suppresses that deferred check

#### Scenario: Numerically current replacement belongs to another family
Test: unit — disposable OpenCode installations.
- **WHEN** the replacement belongs to a different family and numerically meets the captured target
- **THEN** the obsolete target cannot mark it Current; the candidate is invalidated and the existing full check reconciles it
