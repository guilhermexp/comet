## Why
The Chat Trajectory preview (device-local read model of Chat runs, its SQLite
store, capture in the sessions engine, the `WatchTrajectory` /
`RevealTrajectoryRaw` RPCs and the right-pane surface) is no longer wanted.
It is fork-only code (introduced in ad903d87 and 60b4f739, absent upstream),
and keeping it costs capture work on every published event, a background
SQLite writer per profile, and surface area in every upstream sync.

## What Changes
- **BREAKING** Remove the `chat-trajectory-preview` capability entirely:
  - UI: the clock "Trajectory" right-pane button, the `crates/ui/src/trajectory/`
    surface and its capture/demo fixtures (`ZERON_DEMO_TRAJECTORY`).
  - RPC: `WatchTrajectory` and `RevealTrajectoryRaw` methods and their wire
    types (device-local only, never forwarded, so no cross-device peer depends
    on them).
  - Engine: `TrajectoryStore`, capture in `SessionsEngine::publish`, retention
    in `WorkspaceHost`, the Run Journal raw-reveal lookup.
  - Proto: `zeron_proto::trajectory` contracts and projections.
- The engine best-effort deletes the leftover `{store_root}/trajectory.sqlite3`
  (+ `-wal`/`-shm`) at boot, so every device reclaims the space.
- The unstarted change `repair-trajectory-observability-fidelity` is dropped.
- Archived changes, plans and ADRs stay as history; ADR 0004/0005 are marked
  superseded.

## Capabilities
### Removed Capabilities
- `chat-trajectory-preview`

## Impact
`crates/{proto,rpc,engine,ui}`, root docs (`ARCHITECTURE.md`, `CONTEXT.md`,
`DESIGN.md`, `FUNCTIONAL-BASELINE.html`, `docs/PARITY.md`), DOX files of the
touched crates. Chat Transcript, Run Journal, recovery and export behave as
before.
