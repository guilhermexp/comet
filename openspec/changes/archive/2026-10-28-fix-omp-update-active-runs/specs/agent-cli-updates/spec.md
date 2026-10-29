## Purpose

Coordinate device-local agent CLI updates while preserving ongoing agent work and safe admission to the installed version.

## ADDED Requirements

### Requirement: OMP updates preserve active runtime work

A pending OMP update SHALL NOT abort or freeze active turns, tools, accepted steers or background work. Compatible messages and genuine Worker notices SHALL remain deliverable to an active runtime. Installation SHALL wait until the runtime has no remaining work.

#### Scenario: Steering while an update waits

Test: integration — `runtime_survival::omp_update_keeps_active_steers_and_sends_in_the_same_runtime`

- **WHEN** an OMP update waits behind a long active turn and the user steers or sends a compatible message
- **THEN** the existing runtime receives each message once without termination
- **AND** the installer remains blocked while that work is active

#### Scenario: Background work survives a turn boundary

Test: integration — `runtime_survival::omp_update_waits_for_background_work_after_done`

- **WHEN** the foreground turn finishes during a pending OMP update but background work remains
- **THEN** the runtime stays alive and receives Worker notices
- **AND** installation starts only after the work drains

#### Scenario: An accepted continuation owns the next turn

Test: integration — `runtime_survival::omp_update_drains_an_accepted_steer_before_retiring_at_done`

- **WHEN** a steer was accepted before an OMP update and the current turn completes before consuming it
- **THEN** the same runtime consumes that continuation before yielding to installation

### Requirement: OMP installation remains exclusive

New OMP subprocesses SHALL wait while an accepted installation is pending or running. After installation and verification finish, subsequent execution SHALL use the updated installation. Cancellation or failure SHALL release admission without suppressing the update error.

#### Scenario: Old process drains and next process uses the new version

Test: unit — `harness_updates::tests::omp_update_waits_for_long_process_then_next_execution_uses_new_install`

- **WHEN** a fictitious long OMP process holds execution and a fictitious update is requested
- **THEN** the old process continues responding before installation
- **AND** a later execution cannot start until verification completes
- **AND** that later execution observes the new version
