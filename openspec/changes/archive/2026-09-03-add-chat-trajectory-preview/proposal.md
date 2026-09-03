# Change: Add Chat Trajectory Preview

## Why

Comet's synchronized Chat Transcript intentionally omits or sanitizes execution details, while the local Run Journal is a recovery format containing raw sensitive data rather than a product UI contract. Users need a safe, complete, device-local view of every main Chat run, including timing, tool correlation, errors, and usage, after failures and while a run is live.

## Decisions

- **D-01:** Introduce a versioned, profile-local SQLite Trajectory read model in the engine; do not add Trajectory data to synchronized Chat state or export.
- **D-02:** Capture semantic events continuously from the normalized event publication seam, independent of whether the preview is open, and fail open if observability storage degrades.
- **D-03:** Use one bounded nonblocking capture queue and ordered SQLite writer with independent WAL readers so Trajectory cannot block synchronous event publication.
- **D-04:** Store sanitized Payload and Result representations plus owner-checked opaque Run Journal references; resolve one raw field only through an explicit local-only RPC and keep the revealed value in the open view entity.
- **D-05:** Retain local Trajectory history while its Chat exists, preserve it through archive, and remove it by observing the authoritative workspace Chat set so local, synchronized, and Space cascade deletion converge.
- **D-06:** Treat legacy journal events without timestamps as sequence-only; never invent durations, completion state, or run boundaries.
- **D-07:** Open one Trajectory right-pane surface per Chat from a titlebar control beside Capture; the surface owns its watch task and internal responsive inspector.

## What Changes

- Capture normalized main Chat execution records into a sanitized local Trajectory store across all runs on the current device.
- Add versioned record, timing, lane, status, run-boundary, snapshot, delta, and degraded-state contracts.
- Add bounded local-only RPCs for coherent snapshot-plus-delta watching and explicit raw Payload/Result reveal.
- Add a native GPUI Trajectory surface with Input, Model, and Tools timeline lanes; virtualized run/turn/step/event ledger; Duration, Turns, Calls, and Search controls; synchronized selection; stable live scrolling; and an internal Summary/Payload/Result/Schema/Timing inspector.
- Add a titlebar control beside Capture that opens or focuses the selected Chat's single Trajectory surface using the existing right-pane lifecycle.
- Lazily import eligible legacy journals as idempotent sequence-only history and represent incomplete, interrupted, unsettled, degraded, and unavailable states explicitly.
- Add deterministic capture fixtures, focused behavioral tests, real native-surface QA, and DOX/Test Coverage Matrix updates.

## Capabilities

### New Capabilities

- `chat-trajectory-preview`: Always-on device-local main Chat trajectory capture, storage, transport, visualization, inspection, privacy, legacy projection, and right-pane lifecycle.

### Modified Capabilities

None. Existing Chat Transcript, Run Journal recovery, synchronization, export, Capture, and global Details behavior remain unchanged.

## Impact

- `crates/proto`: New typed Trajectory contracts and pure projections.
- `crates/engine`: New local SQLite store, event capture, legacy projection, deletion observation, and raw-source resolution.
- `crates/rpc`: New local-only watch and reveal contracts and handlers.
- `crates/ui`: New Trajectory view/model/timeline/ledger/inspector/toolbar modules plus titlebar and right-pane integration.
- `crates/doc`, sync/edge transport, Chat Transcript Export, and Run Journal recovery semantics remain unchanged.
- Local disk usage grows with retained sanitized trajectory records until Chat deletion; no pruning policy is added in this change.
- Publication remains blocked until the repository's documented push gate is restored or an equivalent is explicitly approved.
