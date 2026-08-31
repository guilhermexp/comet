# Change: Elevate Workers Wait Ceiling to 600s and Bridge Timeout to 900s with Single Source of Truth

## Why

In `crates/workers-unpeel`, the blocking wait ceiling for `wait_for_status` was duplicated across two independent locations (`"maximum": 120` in the MCP tool schema and `.clamp(1, 120)` in `wait_for_status`), risking silent divergence. Furthermore, a 120s ceiling caused frequent timeouts when orchestrators waited on long-running worker tasks, leading to false failures or heavy polling loops.

To fix this:
1. The blocking wait ceiling must be elevated to 600s (10 minutes) and consolidated into a single public constant (`WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`) in `zeron-workers-unpeel`.
2. The host bridge transport timeout `TOOL_CALL_TIMEOUT` in `crates/harness/src/omp/workers_bridge.rs` must be elevated to 900s (15 minutes), maintaining an invariant margin (>= 60s, here 300s) over the 600s tool ceiling.
3. The `timeout_seconds` schema description must be corrected to explain that expiration returns `timed_out: true` with a worker snapshot as a normal read, rather than misleading callers into treating it as a failure or substituting durable checks.

## What Changes

- **Single Constant Ceiling:** Define `pub const WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS: u64 = 600;` in `crates/workers-unpeel/src/lib.rs` and `crates/workers-unpeel/src/controller_mcp.rs` (re-exported).
- **Schema and Clamp Alignment:** Use `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` for `timeout_seconds.maximum`, the `wait_for_status` runtime `.clamp(1, WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS)`, and `action=help` limits.
- **Accurate Description:** Update the `timeout_seconds` field description in the tool schema to explain `timed_out: true` snapshot behavior in two concise sentences.
- **Transport Timeout Elevation:** Update `TOOL_CALL_TIMEOUT` in `crates/harness/src/omp/workers_bridge.rs` from 180s to 900s (15 minutes).
- **Invariant Testing:** Keep `workers_bridge_timeout_strictly_exceeds_tool_blocking_ceiling` verifying that transport timeout strictly exceeds schema maximum with >= 60s margin. Add integration tests verifying schema and clamp derivation from `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`.

## Capabilities

### Modified Capabilities

- `workers-host-bridge`: Elevates the tool blocking ceiling to 600s and transport deadline to 900s with single-source constant derivation and improved timeout semantics documentation.

## Impact

- `crates/workers-unpeel/src/lib.rs`: Exposes `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`.
- `crates/workers-unpeel/src/controller_mcp.rs`: Replaces hardcoded 120s literals with `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` and updates `timeout_seconds` description.
- `crates/workers-unpeel/tests/controller_mcp.rs`: Adds tests asserting schema maximum, help limits, and description semantics match the constant.
- `crates/harness/src/omp/workers_bridge.rs`: Elevates `TOOL_CALL_TIMEOUT` to 900s.
- `crates/harness/tests/omp_rpc.rs`: Validates invariant margin (900s > 600s + 60s).
