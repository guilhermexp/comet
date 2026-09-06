# Change: Fix Worker Completion Wait

## Why

Raising `wait_for_status` to 4h (`2c7e645`) exposed a pre-existing mismatch: the wait predicate compares `wanted` only to literal `activity`/`state`, so `status=completed` never matches a live Worker whose process is still `running` and whose activity is `idle` after the current task episode finished. Parent notifications already treat that episode as `Completed` (Stop hook or `done`+unread, plus quiescent output and generation/task-episode guards). That notification becomes a Steer whose OMP ACK emits `AgentEvent::Steered` while the host wait tool is still in flight, so the engine closes the parent segment without `toolResult` or resumption. Observed 2026-09-06 on Chat `ea8c8913-dc63-4a5f-b0c4-6919df9c5eb5`, Worker `cfa82441-b8de-4198-bc72-ebb23a83f2e8`: wait at 01:16:49Z, closeout 01:25:21Z, Steered 01:25:30Z, still Working/Marinating at 01:38:23Z with the tool failed.

## What Changes

- `wait_for_status(status=completed)` SHALL match authoritative completion of the **current** task episode even when the Worker process is live and activity is `idle`.
- Matching SHALL reuse existing parent-notification completion evidence (Stop/`done`+unread, output quiescence, runtime generation, task-episode guards, `acknowledged_completed_episode` latch). No parallel detector. Idle is not completed.
- OMP SHALL NOT send a steer to the child or emit `AgentEvent::Steered` while a host tool is pending. The steer is queued, the host `toolResult` (or cancel error) is delivered first, then the steer prompt is processed exactly once.
- The 4h wait ceiling, per-call bridge margin, concurrent/cancellable dispatch, and existing timeout/cancel/`next` guidance stay unchanged.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workers-host-bridge`: current-episode completion satisfies `wait_for_status(completed)` on a live/idle Worker; steer ACK does not fabricate a run frontier while a host tool is pending.

## Impact

- `crates/workers-unpeel/src/controller_mcp.rs`, `src/parent_notifications.rs`, `src/lib.rs`, `tests/controller_mcp.rs`, `tests/parent_notifications.rs`, `AGENTS.md`.
- `crates/harness/src/omp/{mod,workers_bridge}.rs`, `tests/omp_rpc.rs`, `tests/fixtures/fake-omp-rpc.sh`, `tests/fixtures/fake-workers-controller-mcp.sh`, `AGENTS.md`.
- Engine `sessions.rs` is unchanged: `Steered` remains a real frontier once the harness emits it after the host tool is consumed.
