# Tasks: Preserve Workers Wait Ceiling at 120s and Elevate Bridge Timeout to 900s

- [x] Write TDD RED tests asserting ceiling derivation and updated description <!-- id: 0 -->
- [x] Implement `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS = 120` in `zeron-workers-unpeel` and use in schema, clamp, and help <!-- id: 1 -->
- [x] Update `timeout_seconds` description in `controller_mcp.rs` <!-- id: 2 -->
- [x] Elevate `TOOL_CALL_TIMEOUT` to 900s in `zeron-harness` <!-- id: 3 -->
- [x] Verify all test suites and invariants pass (TDD GREEN) <!-- id: 4 -->
- [x] Validate OpenSpec change strictly <!-- id: 5 -->
- [x] Add the serial-controller ceiling regression guard and capture its RED output <!-- id: 6 -->
- [x] Document serial dispatch, blocked stop/archive actions, discarded cancellation, and concurrency prerequisite <!-- id: 7 -->
- [x] Declare the prerequisite archive order in the proposal <!-- id: 8 -->
