# Change: Fix Dead Antigravity Error Variant and Workers Bridge Timeout Collision

## Why

Two independent contract defects exist in the current codebase:
1. **Unreachable Error Variant (Defect A):** In `crates/engine/src/antigravity_usage.rs`, `AntigravityUsageError::RefreshConfiguration` became unreachable after commit `5180c17e`. Because `can_refresh` is false whenever OAuth client variables are missing, unrenewable expired credentials legitimately yield `CredentialsExpired`. The leftover `RefreshConfiguration` variant retained dead code and was annotated with `#[allow(dead_code)]`.
2. **Workers Bridge Deadline Collision (Defect B):** In `crates/harness/src/omp/workers_bridge.rs`, `TOOL_CALL_TIMEOUT` is set to 120s (`Duration::from_secs(2 * 60)`), while `crates/workers-unpeel/src/controller_mcp.rs` allows `wait_for_status` to block for up to 120s (`timeout_seconds` max: 120). When a worker wait nears the 120s limit, the transport deadline collides with the tool wait, causing `request_bounded` to cancel the call and return a false-negative timeout error (`tools/call: request timed out`) even though the worker is healthy.

Combining both fixes into a single change cleanly addresses these contract defects without unnecessary fragmentation.

## What Changes

- **Defect A:** Remove the dead `RefreshConfiguration` enum variant from `AntigravityUsageError` in `crates/engine/src/antigravity_usage.rs`, ensuring all remaining error variants are reachable and covered.
- **Defect B:** Elevate `TOOL_CALL_TIMEOUT` in `crates/harness/src/omp/workers_bridge.rs` from 120s to 180s (3 minutes), establishing an explicit 60s margin above the tool schema's 120s maximum blocking wait (`timeout_seconds.maximum`), justified by a comment explaining coverage for JSON-RPC IPC round-trip, serialization, and host scheduling latency.
- **Invariance Pinning:** Add an integration test deriving values from the real tool schema (`controller_mcp`) and transport constant (`TOOL_CALL_TIMEOUT`) to prevent regression of the deadline margin.

## Capabilities

### New Capabilities

- `workers-host-bridge`: Transport timeout contract guaranteeing that the host tool execution deadline strictly exceeds the worker controller's maximum blocking ceiling with an explicit round-trip margin.

### Modified Capabilities

- `antigravity-managed-usage`: Removes the obsolete `RefreshConfiguration` error variant, ensuring diagnostic errors accurately reflect expired unrenewable credentials (`CredentialsExpired`) or upstream refresh failures.

## Impact

- `crates/engine/src/antigravity_usage.rs`: Removed `AntigravityUsageError::RefreshConfiguration`.
- `crates/harness/src/omp/workers_bridge.rs`: Elevated `TOOL_CALL_TIMEOUT` to 180s with justification comment and public visibility for invariant verification.
- `crates/harness/Cargo.toml`: Added `zeron-workers-unpeel.workspace = true` to `[dev-dependencies]` for cross-contract testing.
- `crates/harness/tests/omp_rpc.rs`: Added pinned invariant test comparing real transport timeout against the tool schema maximum.
