## Context

The host engine owns the Live child and its control channel. UI Chat selection currently sends `StopLiveVoice`, even though delegation execution and backend observation continue in `SessionsEngine`; a completed answer therefore persists to the Chat after the Live transport has already been released. Separately, OMP keeps a completed steerable run handle parked for warm reuse and marks its session `Idle`, while Live eligibility currently checks only whether that handle exists.

The incident journal proves the delegated backend emitted the full visible answer and terminal `Done` with no error. The OMP session log proves normal backend disposal. The missing speech therefore occurs after UI-triggered Live teardown, not in backend generation or transcript persistence.

## Goals / Non-Goals

**Goals:**

- Make the engine-owned Live lifecycle independent of selected UI surface.
- Preserve the existing rule that a genuinely working or input-blocked backend prevents Live start.
- Reuse the parked OMP session instead of terminating it solely to permit Live restart.
- Cover the incident terminal shape where `Done.result` is absent and visible deltas contain the final answer.

**Non-Goals:**

- Change OMP's Live RPC wire format or speech model.
- Persist casual Live transcript or audio.
- Allow a competing durable command to run while Live remains active.
- Add a second Live call or global multi-call controls.

## Decisions

### D1: UI navigation does not own Live teardown

Remove the `StopLiveVoice` side effect from Chat selection. Explicit End/Escape, competing command preparation, engine shutdown, transport failure, and app quit remain existing lifecycle boundaries. This keeps the selected view a projection of engine state rather than an owner of the Live process.

Alternative: buffer delegation finals after navigation and replay them on return. Rejected because the Live child has already released microphone/playback, replay ordering is ambiguous, and it duplicates lifecycle state outside the engine.

### D2: Eligibility follows session status

Replace run-handle presence in the Live precondition with the existing session status. `Working` and `AwaitingInput` reject Live start; `Idle`, terminal states, and absence permit it. The parked handle remains available for OMP steering/session reuse.

Alternative: terminate every completed OMP handle before Live restart. Rejected because it discards the warm-reuse optimization and adds unnecessary process churn.

### D3: Regress at behavioral boundaries

Extend engine e2e coverage to use streamed visible text plus terminal `Done` without a result, then end and restart Live while the completed run remains parked. UI selection policy is validated by the UI unit suite after removing the obsolete stop policy; the actual gpui surface is smoke-checked in the development app because gpui rendering has no harness.

## Risks / Trade-offs

- Live audio can continue while its originating Chat is not selected. This is intentional; the user must use the existing global/app lifecycle controls or return to the Chat to end it.
- Starting a different durable command still ends Live before execution, preventing concurrent ownership conflicts.
- Status snapshots are eventually delivered. Start-time validation remains authoritative and reads the host engine's current session status rather than trusting stale UI availability.
