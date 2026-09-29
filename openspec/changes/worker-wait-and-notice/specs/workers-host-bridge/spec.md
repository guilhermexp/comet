## ADDED Requirements

### Requirement: wait_for_status status is a validated, documented contract

The Workers controller SHALL accept for `wait_for_status` only `completed` and the lifecycle values that `list_workers`/`inspect_worker` report for `state` and `activity`, compared case-insensitively. Any other value SHALL be rejected with an error that lists the accepted values, without waiting. The tool schema, the tool description and `action=help` SHALL name `completed` as the status for "the worker finished its task", and SHALL state that `idle` matches any pause (including a worker waiting on its own subagents) and `exited` matches only a dead process.

#### Scenario: Unknown status is rejected without waiting
- Test: integration — controller MCP integration test calling `wait_for_status` with an unknown status against a live worker and asserting an immediate error listing the accepted values.

- **GIVEN** a live worker
- **WHEN** the orchestrator calls `wait_for_status` with `status` set to a value outside the accepted set
- **THEN** the call returns an error naming the accepted values
- **AND** it returns without blocking for `timeout_seconds`

#### Scenario: Schema and help document completed as the finish target
- Test: integration — controller MCP integration test inspecting the tool schema `status` description, the tool description and `action=help`.

- **WHEN** the tool schema, tool description and `action=help` are read
- **THEN** each names `completed` as the status for a finished task
- **AND** each states that `idle` and `exited` do not mean the task finished

### Requirement: A lifecycle wait ends when the current episode completes

A `wait_for_status` on any accepted status other than `completed` SHALL also return, within one poll tick of completion becoming observable, when the worker's current task episode transitions to completed during the wait. The result SHALL say the episode completed and SHALL NOT claim the requested status matched. A completion that already held when the wait started SHALL NOT end a wait on another status.

#### Scenario: Waiting on exited returns when a live worker finishes
- Test: integration — controller MCP integration test with a live idle worker whose current episode gains Stop evidence during a `wait_for_status(exited)`.

- **GIVEN** a live worker with an active task episode and a pending `wait_for_status` on `exited`
- **WHEN** the current episode completes (Stop evidence and quiescent output) while the process stays alive
- **THEN** the wait returns within one poll tick with a result that marks the episode as completed
- **AND** the result does not report `exited` as matched

#### Scenario: Prior completion does not end a new lifecycle wait
- Test: integration — controller MCP integration test starting `wait_for_status(working)` on a worker whose current episode was already completed before the call.

- **GIVEN** a worker whose current episode completed before the call
- **WHEN** the orchestrator waits on `working` with a short timeout
- **THEN** the wait does not return early because of that prior completion

### Requirement: A worker notification ends the parent's pending wait

When a `[worker-task-notification]` is queued into a parent chat whose OMP turn has a pending Workers `wait_for_status` host tool call, the harness SHALL end that wait promptly instead of holding the notification until the wait times out. The wait's tool result SHALL be delivered first and SHALL state that it was interrupted by a worker notification (not a transport failure). The notification prompt SHALL then be processed exactly once. Steers that are not worker notifications, and pending host tools other than `wait_for_status`, SHALL keep the existing queue-until-result behaviour.

#### Scenario: Notification during a pending wait is delivered without waiting for the timeout
- Test: integration — harness integration test in `omp_rpc` with the fake OMP and fake Workers controller: a long `wait_for_status` is pending when a worker-notification steer arrives.

- **GIVEN** an OMP turn with a pending `wait_for_status` whose timeout is far in the future
- **WHEN** a `[worker-task-notification]` steer is queued for that chat
- **THEN** the wait's tool result, marked as interrupted by a worker notification, is delivered before the steer
- **AND** the steer prompt is delivered once, well before the wait's timeout

#### Scenario: Ordinary steer keeps queue-until-result
- Test: integration — harness integration test in `omp_rpc` sending a non-notification steer during a pending `wait_for_status`.

- **GIVEN** an OMP turn with a pending `wait_for_status`
- **WHEN** a steer that is not a worker notification arrives
- **THEN** the wait is not interrupted and the steer is delivered after the wait's own result

### Requirement: Claude Workers expose no suggested prompt in their composer

Claude Code Workers SHALL be launched with prompt suggestions disabled (`CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=false` or an equivalent documented Claude Code control), so the idle composer captured in `read_output`, `inspect_worker` and the notification output tail carries no suggested next prompt. Permission dialogs and other real input requests SHALL remain in the captured output.

#### Scenario: Claude Worker launch disables prompt suggestions
- Test: unit — unit test on the Claude Worker launch command/environment asserting prompt suggestions are disabled.

- **WHEN** a Claude Code Worker launch command and environment are built
- **THEN** they disable Claude Code prompt suggestions

#### Scenario: Blocked Claude Worker still shows its permission prompt
- Test: unit — parent-notification or controller output test with a captured Claude permission dialog viewport, asserting the dialog text is present in the output tail.

- **GIVEN** a Claude Worker viewport showing a permission dialog
- **WHEN** the notification output tail is built
- **THEN** the dialog text is present in the tail
