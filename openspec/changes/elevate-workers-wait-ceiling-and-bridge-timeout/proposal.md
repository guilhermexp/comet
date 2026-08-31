# Change: Preserve Workers Wait Ceiling at 120s and Elevate Bridge Timeout to 900s with Single Source of Truth

## Why

In `crates/workers-unpeel`, the blocking wait ceiling for `wait_for_status` was duplicated across two independent locations (`"maximum": 120` in the MCP tool schema and `.clamp(1, 120)` in `wait_for_status`), risking silent divergence. The bridge transport deadline also needed substantially more headroom than the blocking budget to avoid false transport timeouts.

To fix this:
1. The blocking wait ceiling remains 120s and is consolidated into one public constant (`WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`) in `zeron-workers-unpeel`; raising it would block the serial controller's stop and archive actions.
2. The host bridge transport timeout `TOOL_CALL_TIMEOUT` in `crates/harness/src/omp/workers_bridge.rs` is elevated to 900s (15 minutes), maintaining a 780s margin over the 120s tool ceiling.
3. The `timeout_seconds` schema description explains that expiration returns `timed_out: true` with a worker snapshot as a normal read, rather than misleading callers into treating it as a failure or substituting durable checks.

## What Changes

- **Single Constant Ceiling:** Define `pub const WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS: u64 = 120;` in `crates/workers-unpeel/src/controller_mcp.rs` and re-export it from `lib.rs`.
- **Schema and Clamp Alignment:** Use `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` for `timeout_seconds.maximum`, the `wait_for_status` runtime `.clamp(1, WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS)`, and `action=help` limits.
- **Accurate Description:** Update the `timeout_seconds` field description in the tool schema to explain `timed_out: true` snapshot behavior in two concise sentences.
- **Transport Timeout Elevation:** Update `TOOL_CALL_TIMEOUT` in `crates/harness/src/omp/workers_bridge.rs` from 180s to 900s (15 minutes).
- **Invariant Testing:** Keep `workers_bridge_timeout_strictly_exceeds_tool_blocking_ceiling` verifying that transport timeout strictly exceeds schema maximum with >= 60s margin. Add integration tests verifying schema and clamp derivation from `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`, plus a guard that caps the wait at 120s while controller dispatch remains serial.

## Capabilities

### Modified Capabilities

- `workers-host-bridge`: Preserves the tool blocking ceiling at 120s and elevates the transport deadline to 900s with single-source constant derivation and improved timeout semantics documentation.

## Impact

- `crates/workers-unpeel/src/lib.rs`: Exposes `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS`.
- `crates/workers-unpeel/src/controller_mcp.rs`: Replaces duplicated ceiling literals with `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` and updates the `timeout_seconds` description.
- `crates/workers-unpeel/tests/controller_mcp.rs`: Adds tests asserting schema maximum, help limits, and description semantics match the constant.
- `crates/harness/src/omp/workers_bridge.rs`: Elevates `TOOL_CALL_TIMEOUT` to 900s.
- `crates/harness/tests/omp_rpc.rs`: Validates invariant margin (900s > 120s + 60s).
- **Archive order:** `fix-dead-antigravity-error-and-workers-timeout` must be archived before this change because it introduces the requirement modified by this delta.
