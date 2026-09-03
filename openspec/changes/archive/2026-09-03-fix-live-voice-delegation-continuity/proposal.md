## Why

Live Voice currently ends when the user navigates away from its Chat, so an in-flight delegated run can finish durably without its final answer reaching speech. The completed OMP run then remains parked for warm reuse, but Comet mistakes that idle handle for active coding work and prevents Live Voice from restarting.

## What Changes

- Keep a host-local Live Voice call active while the user selects another Chat or clears the current Chat surface; explicit End, competing durable commands, transport failure, engine shutdown, and app quit still stop it.
- Deliver delegated progress and the final backend answer after UI navigation because the Live child remains owned by the engine rather than the selected view.
- Treat only `Working` and `AwaitingInput` backend states as an active coding run for Live Voice eligibility; an `Idle` parked OMP session remains reusable and does not block restart.
- Add regressions for final delegation delivery with a null terminal result and for restarting Live Voice while the completed OMP run is parked.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `omp-live-voice`: Clarify navigation-independent lifecycle, delegated final speech continuity, and restart eligibility after a completed parked OMP run.

## Impact

- `crates/ui/src/state.rs`: Chat selection no longer stops Live Voice.
- `crates/engine/src/sessions.rs`: Live eligibility follows runtime status instead of run-handle presence.
- `crates/engine/tests/e2e.rs`: delegation continuity and restart regression coverage.
- `openspec/specs/omp-live-voice/spec.md` after archive: updated lifecycle and eligibility requirements.
- No wire-format, CRDT, edge, or persisted-data changes.
