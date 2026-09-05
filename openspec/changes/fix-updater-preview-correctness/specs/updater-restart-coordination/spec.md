## ADDED Requirements

### Requirement: Atomic Restart Authorization Against Work Admission

The engine and updater SHALL coordinate restart authorization atomically with run and terminal admission. When a restart is authorized, new runs and terminals SHALL be rejected, and restart authorization SHALL only succeed when no runs or terminals are currently active. Staging of updates SHALL be permitted while work is active.

#### Scenario: Work rejected after restart authorization

Test: `tests::terminal_open_rejected_when_restart_authorized` and `tests::dispatch_rejected_when_restart_authorized` in `zeron-engine`

- **WHEN** the updater authorizes a service restart
- **THEN** attempts to dispatch new runs or open new terminals are rejected
- **AND** the lockout remains active during the 800ms restart window until process exit

#### Scenario: Admission reservation blocks restart authorization during setup

Test: `tests::dispatch_admission_reserved_before_setup_blocks_restart` and `tests::terminal_admission_reserved_before_setup_blocks_restart` in `zeron-engine`

- **WHEN** work admission (run dispatch or terminal open) is reserved and in-flight across awaits
- **THEN** restart authorization is rejected with `WorkActive`
- **AND** once setup completes and registers work into active state, restart authorization continues to be rejected

#### Scenario: Single restart ownership and failure rollback

Test: `tests::restart_gate_prevents_double_authorization_and_stale_release`, `tests::apply_authorized_rolls_back_on_restart_task_failure_or_cancellation`, and `tests::auto_apply_admits_work_during_staging_and_blocks_restart_until_quiet` in `zeron-update`

- **WHEN** restart authorization is acquired, concurrent authorizations are rejected (preventing double ownership)
- **AND** if the restart task fails or is cancelled, the authorization is released and work admission is reopened
- **AND** dropping a stale or uncommitted authorization does not release an active authorization held by another owner
### Requirement: Persistent Shutdown Even Without Receiver

`Updater::shutdown` SHALL reliably terminate the background check loop within a timeout in a `current_thread` runtime even when called immediately after `spawn`, and SHALL be idempotent on repeated invocation.

#### Scenario: Immediate shutdown in current_thread runtime terminates

Test: `tests::shutdown_immediately_after_spawn_terminates`

- **WHEN** `Updater::spawn` is called and `Updater::shutdown` is awaited immediately in a `current_thread` runtime
- **THEN** the shutdown future completes within timeout
- **AND** no background polling task remains alive

#### Scenario: Repeated shutdown is idempotent

Test: `tests::repeated_shutdown_is_idempotent`

- **WHEN** `Updater::shutdown` is called multiple times consecutively
- **THEN** all calls succeed and the loop does not hang or restart
