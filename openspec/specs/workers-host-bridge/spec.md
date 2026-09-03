# workers-host-bridge Specification

## Purpose

Host bridge transport deadlines, concurrent and cancellable worker controller dispatch, and orchestrator-owned wait ceilings.

## Requirements

### Requirement: Workers host tool transport deadline exceeds tool blocking budget

The host bridge transport deadline for a Workers tool call SHALL be derived per call: for `wait_for_status` it SHALL be the requested `timeout_seconds` plus a margin of at least 60 seconds for JSON-RPC IPC round-trip, serialization and host scheduling; for every other action it SHALL be `TOOL_CALL_TIMEOUT` (900s). The margin therefore holds for any wait the orchestrator requests, up to the controller ceiling.

#### Scenario: Tool call timeout exceeds wait_for_status maximum ceiling
Test: harness integration test deriving the maximum wait duration from the controller MCP tool schema and asserting the transport timeout exceeds it with a round-trip margin.

- **WHEN** the Workers controller MCP schema advertises a maximum blocking wait (`timeout_seconds.maximum`)
- **THEN** the bridge transport deadline derived for `wait_for_status` at that maximum exceeds it by at least 60 seconds

#### Scenario: Wait deadline follows the requested timeout
Test: harness unit test on `call_timeout_for` deriving the deadline from `wait_for_status` arguments at the controller ceiling.

- **WHEN** the orchestrator calls `wait_for_status` with `timeout_seconds` equal to the controller ceiling
- **THEN** the bridge deadline exceeds that value by at least 60 seconds

#### Scenario: Other actions keep the fixed transport deadline
Test: harness unit test on `call_timeout_for` with a non-wait action.

- **WHEN** the tool call is any action other than `wait_for_status`
- **THEN** the bridge deadline is `TOOL_CALL_TIMEOUT`

### Requirement: Single constant for Workers status wait ceiling and clear timeout semantics

The Workers controller MCP tool schema, `action=help` limits, and the runtime clamp for `wait_for_status` SHALL derive their maximum blocking wait from a single public constant (`WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` = 14400, four hours) in `zeron-workers-unpeel`; the orchestrator chooses any `timeout_seconds` up to it and the default remains 30 seconds. The controller SHALL dispatch requests concurrently so a pending wait never blocks `stop_worker`, `archive_worker` or `ping`, SHALL honour `notifications/cancelled` by interrupting the pending wait without sending a response, and SHALL cancel pending waits when its input closes. A `timed_out: true` result SHALL carry a `next` field stating that the caller may wait again with a timeout sized to the work or end the turn and receive `[worker-task-notification]`.

#### Scenario: Schema, help limits, and runtime clamp derive from single constant
Test: controller MCP integration test asserting schema `maximum`, `help` limits, and runtime clamp match `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`.

- **WHEN** the controller MCP tool schema is inspected
- **THEN** `timeout_seconds.maximum` equals `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` (14400)
- **AND** `action=help` reports `limits.wait_seconds` equal to `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`

#### Scenario: Pending wait does not block the control channel
Test: controller MCP integration test driving `serve` with a blocking request followed by `ping` and a cancellation.

- **GIVEN** a request is pending in `serve`
- **WHEN** a `ping` arrives on the same channel
- **THEN** the `ping` is answered before the pending request completes

#### Scenario: Cancellation interrupts a pending wait without a response
Test: controller MCP integration tests on `serve` and on `wait_until` with a cancel flag.

- **WHEN** `notifications/cancelled` names a pending request id
- **THEN** the pending wait stops within one poll tick and no response is written for that id

#### Scenario: Timeout result carries next-step guidance
Test: controller MCP unit test on `wait_until` expiring against a running worker.

- **WHEN** a `wait_for_status` expires with the worker still running
- **THEN** the result has `timed_out: true`, the worker snapshot and a `next` string naming `[worker-task-notification]` and the option to wait again

### Requirement: Native orchestrator runtimes receive a matching MCP client deadline

When the Workers controller MCP is mounted into a native orchestrator runtime, the harness SHALL configure that runtime's MCP tool-call deadline to `WORKERS_CLIENT_DEADLINE_SECONDS` = `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS + 60`: Claude via the `MCP_TOOL_TIMEOUT` environment variable (milliseconds) and Codex via the `mcp_servers.comet-workers.tool_timeout_sec` override. The harness constant SHALL be pinned to the controller constant by test.

#### Scenario: Claude process deadline
Test: harness unit test on `build_command` with Workers MCP enabled.

- **WHEN** the Claude harness builds a command with Workers MCP enabled
- **THEN** the process environment carries `MCP_TOOL_TIMEOUT` equal to `WORKERS_CLIENT_DEADLINE_SECONDS * 1000`

#### Scenario: Codex override deadline
Test: harness unit test on Codex overrides with Workers MCP enabled.

- **WHEN** the Codex harness mounts the Workers MCP
- **THEN** the overrides contain `mcp_servers.comet-workers.tool_timeout_sec=<WORKERS_CLIENT_DEADLINE_SECONDS>`
