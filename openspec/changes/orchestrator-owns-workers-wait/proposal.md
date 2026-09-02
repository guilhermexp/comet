# Change: Orchestrator Owns the Workers Wait Duration

## Why

`wait_for_status` is capped at 120s while a delegated worker routinely runs for one to three hours. The orchestrator, who knows how long the work should take, cannot express that: every expiry costs a full model turn with the whole context re-sent, so a long run turns into ~100 polling calls per attempt (observed on `add-chat-trajectory-preview` F2, 2026-09-01). The cap existed because `run_stdio` dispatched serially and discarded `notifications/cancelled`: a long wait would have blocked `stop_worker`/`archive_worker` and could not be interrupted. Those two prerequisites are what this change removes; the ceiling then follows.

## What Changes

- **Concurrent, cancellable controller**: `run_stdio` becomes a thin wrapper over `serve(reader, writer, handler)` that runs each JSON-RPC request on its own thread, tracks in-flight request ids, honours `notifications/cancelled` by flipping the request's cancel flag, and sends no response for a cancelled request. `wait_for_status` checks the flag every poll tick. EOF cancels every pending wait so the sidecar exits with its client.
- **Ceiling owned by the orchestrator**: `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS` rises to 4h (14400s); the default stays 30s. `timed_out` results carry a `next` field telling the caller to wait again with a timeout sized to the work or end the turn and receive `[worker-task-notification]`.
- **Transport deadline derived per call**: the OMP bridge computes `call_timeout_for(arguments)` = `timeout_seconds + 60s` for `wait_for_status`, `TOOL_CALL_TIMEOUT` (900s) otherwise, so the transport margin holds for any wait by construction.
- **Native runtimes get a matching client deadline**: Claude receives `MCP_TOOL_TIMEOUT` (ms) and Codex `mcp_servers.comet-workers.tool_timeout_sec`, both `WORKERS_CLIENT_DEADLINE_SECONDS` = ceiling + 60, pinned to the controller constant by test.

## Capabilities

### Modified Capabilities

- `workers-host-bridge`: concurrent cancellable controller, 4h orchestrator-owned ceiling with `next` guidance, per-call transport deadline, native client deadlines.

## Decisions

- **D-01 — Ceiling is a transport sanity bound, not a policy.** 4h covers a long worker; the orchestrator picks any value up to it. Ending the turn remains free: the parent notification already arrives.
- **D-02 — Cancelled requests get no response** (MCP cancellation contract); the OMP bridge already drops the pending entry on cancel, native clients ignore late responses.
- **D-03 — Harness keeps no production dependency on `zeron-workers-unpeel`.** `WORKERS_CLIENT_DEADLINE_SECONDS` is a harness constant asserted equal to `WAIT_FOR_STATUS_MAX_TIMEOUT_SECONDS + 60` by a harness test (dev-dependency already present).
- **D-04 — ACP adapters (`claude-agent-acp`, `codex-acp`, `pi-acp`) keep their own MCP client timeouts.** Out of scope here; recorded as not proven.

## Archive order

`workers-host-bridge` has no canonical spec yet: `fix-dead-antigravity-error-and-workers-timeout` (which introduces it) cannot archive before `add-antigravity-usage`, and `elevate-workers-wait-ceiling-and-bridge-timeout` depends on it in turn. This delta is therefore written as ADDED with the full requirement text; it supersedes the two requirements of `elevate-…` and the ceiling scenario they pinned at 120s. Archive `add-antigravity-usage` → `fix-dead-…` → `elevate-…` → this change, converting this delta to MODIFIED if the spec exists by then.

## Impact

- `crates/workers-unpeel/src/controller_mcp.rs`, `src/lib.rs`, `tests/controller_mcp.rs`, `AGENTS.md`.
- `crates/harness/src/omp/workers_bridge.rs`, `src/claude/mod.rs`, `src/codex/mod.rs`, `src/lib.rs`, `tests/omp_rpc.rs`, `AGENTS.md`.
