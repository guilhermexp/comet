# Tasks: Elevate Workers Wait Ceiling to 600s and Bridge Timeout to 900s

- [x] Write TDD RED tests asserting 600s ceiling, constant derivation, and updated description <!-- id: 0 -->
- [x] Implement `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS = 600` in `zeron-workers-unpeel` and use in schema, clamp, and help <!-- id: 1 -->
- [x] Update `timeout_seconds` description in `controller_mcp.rs` <!-- id: 2 -->
- [x] Elevate `TOOL_CALL_TIMEOUT` to 900s in `zeron-harness` <!-- id: 3 -->
- [x] Verify all test suites and invariants pass (TDD GREEN) <!-- id: 4 -->
- [x] Validate OpenSpec change strictly <!-- id: 5 -->
