## Why

Live Voice is unavailable while an OMP Chat Session is working, preventing the user from asking by voice what the coding agent is doing or whether it has encountered a problem. Removing only that availability gate would leave Live unaware of the already-running stream, so the call also needs silent operational context from the host engine.

## What Changes

- Allow Live Voice to start for an otherwise eligible local OMP Chat while its Session is `Working` or `AwaitingInput` when OMP advertises session-context support.
- Feed the Live frontend a bounded, display-safe operational projection of the originating Chat's current Session and visible stream without initiating speech or mutating the Chat.
- Require Live to ask for explicit voice confirmation before emitting a host delegation that changes or adds work during an active run.
- Route a confirmed instruction through the active run's existing steer mailbox; if the run settles during confirmation, preserve the instruction as exactly one ordinary durable turn.
- Keep idle Live Voice compatible with older OMP versions that advertise basic Live support but not active-run session context.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `omp-live-voice`: Extend availability, transient context, and delegation routing to cover a Live call started while the Chat Session is already active.

## Impact

- OMP RPC Live protocol: additive `liveVoiceSessionContext` capability and silent session-context command.
- `crates/harness`: typed capability probing and session-context control.
- `crates/engine`: active-Session availability, latest-value operational projection, and confirmed steer routing.
- `crates/proto` and `crates/ui`: remove the ordinary active-run unavailable state while preserving old-OMP update guidance.
- `openspec/specs/omp-live-voice/spec.md` after archive: active-run observation and confirmed-instruction requirements.
- No audio, casual Live transcript, reasoning delta, raw Run Journal payload, or new persisted field enters Comet RPC, CRDT, edge sync, or logs.
