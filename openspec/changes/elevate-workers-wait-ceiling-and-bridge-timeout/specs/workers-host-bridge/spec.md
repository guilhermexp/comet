## MODIFIED Requirements

### Requirement: Workers host tool transport deadline exceeds tool blocking budget

The host bridge transport timeout for executing Workers tool calls SHALL strictly exceed the maximum allowable blocking duration defined in the Workers controller MCP tool schema, preserving an explicit margin of at least 60 seconds (with `TOOL_CALL_TIMEOUT` at 900s for a 120s tool ceiling) for JSON-RPC IPC round-trip, serialization, and host scheduling latency.

#### Scenario: Tool call timeout exceeds wait_for_status maximum ceiling
Test: harness integration test deriving the maximum wait duration from the controller MCP tool schema and asserting the transport timeout exceeds it with a round-trip margin.

- **WHEN** the Workers controller MCP schema advertises a maximum blocking wait (`timeout_seconds.maximum`)
- **THEN** `TOOL_CALL_TIMEOUT` on the host bridge is configured to a duration greater than that maximum (900 seconds)
- **AND** the difference between `TOOL_CALL_TIMEOUT` and the tool maximum is at least 60 seconds

## ADDED Requirements

### Requirement: Single constant for Workers status wait ceiling and clear timeout semantics

The Workers controller MCP tool schema, `action=help` limits, and the runtime clamp for `wait_for_status` SHALL derive their maximum blocking wait from a single public constant (`WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS = 120`) in `zeron-workers-unpeel` to eliminate literal duplication and prevent drift. While `run_stdio` dispatch remains serial and waits are not interruptible, that constant SHALL NOT exceed 120 seconds. The schema description SHALL clarify that expiration yields `timed_out: true` with a worker snapshot as a normal read, and that long waits do not replace a durable completion check.

#### Scenario: Schema, help limits, and runtime clamp derive from single constant
Test: controller MCP integration test asserting schema `maximum`, `help` limits, and runtime clamp match `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`.

- **WHEN** the controller MCP tool schema is inspected
- **THEN** `timeout_seconds.maximum` equals `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` (120)
- **AND** `action=help` reports `limits.wait_seconds` equal to `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` (120)

#### Scenario: Serial controller wait ceiling preserves control-channel availability
Test: controller MCP integration test asserting the public ceiling remains at most 120 seconds.

- **GIVEN** `run_stdio` dispatches requests serially and a blocking wait prevents stop and archive actions
- **THEN** `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` is at most 120

#### Scenario: Timeout description clarifies normal snapshot read
Test: controller MCP unit test verifying the `timeout_seconds` property description text.

- **WHEN** inspecting the `timeout_seconds` description in the tool definition
- **THEN** the text states in at most two sentences that a blocking wait returns `timed_out: true` with a worker snapshot on deadline expiration and is a normal read rather than a failure
- **AND** states that long waits do not replace a durable completion check
