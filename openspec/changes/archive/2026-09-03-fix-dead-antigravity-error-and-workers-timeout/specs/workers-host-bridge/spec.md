## ADDED Requirements

### Requirement: Workers host tool transport deadline exceeds tool blocking budget

The host bridge transport timeout for executing Workers tool calls SHALL strictly exceed the maximum allowable blocking duration defined in the Workers controller MCP tool schema, preserving an explicit margin for JSON-RPC IPC round-trip, serialization, and host scheduling latency.

#### Scenario: Tool call timeout exceeds wait_for_status maximum ceiling
Test: harness integration test deriving the maximum wait duration from the controller MCP tool schema and asserting the transport timeout exceeds it with a round-trip margin.

- **WHEN** the Workers controller MCP schema advertises a maximum blocking wait (`timeout_seconds.maximum`)
- **THEN** `TOOL_CALL_TIMEOUT` on the host bridge is configured to a duration greater than that maximum
- **AND** the difference between `TOOL_CALL_TIMEOUT` and the tool maximum is at least 30 seconds
