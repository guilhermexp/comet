## Why

Two real OMP Worker launches opened a process but timed out delivering `initial_text`; the same worker executed the task after a separate `send_text`. The Comet controller waits for terminal readiness and mistakes retained MCP startup text for ongoing boot. Orchestrator.dev avoids this OMP failure by passing the initial prompt through the runtime's native startup interface.

## What Changes

- Deliver an OMP Worker's initial brief through its native startup path, without requiring viewport readiness or a second controller call.
- Preserve the chosen preset, checkout, session ownership, parent task tracking and explicit delivery outcome. Do not add approval-bypass flags.
- Prevent duplicate initial submission, unsafe shell interpolation and replay of the initial task on restart.
- Retain the existing guarded interactive delivery path for other runtimes; no speculative provider sweep.
- Prove the behavior with a focused regression and a real isolated controller/OMP launch, then update the installed Comet without interrupting unrelated sessions.

## Capabilities

### New Capabilities
- `worker-initial-briefing`: reliable native startup delivery and truthful launch outcomes for OMP Workers.

### Modified Capabilities

None.

## Impact

Comet's `crates/workers-unpeel` controller and tests; the vendored Unpeel launch/runtime boundary only where native startup transport requires it, with provenance and local DOX updates. Public `workers.launch_worker` arguments remain compatible. Orchestrator.dev is a read-only reference. Graft configuration repair, notification output, admin/cards work and other current Comet changes are out of scope.
