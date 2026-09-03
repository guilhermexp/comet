## 1. OMP Live protocol

- [x] 1.1 Add the additive `liveVoiceSessionContext` capability and typed `live_append_session_context` RPC command.
- [x] 1.2 Forward validated session context through OMP's existing commentary `session.context.append` primitive without speech or delegation side effects.
- [x] 1.3 Update Live instructions to keep operational context silent and require confirmation before active-run delegation.
- [x] 1.4 Cover the RPC command, lifecycle validation, wire shape, and silent controller behavior with focused OMP tests.

## 2. Comet harness boundary

- [x] 2.1 Replace Boolean Live probing with explicit basic/session-context support and parse the additive OMP capability.
- [x] 2.2 Add `AppendSessionContext` to the typed Live control path and serialize it to the new OMP RPC command.
- [x] 2.3 Cover capability compatibility, blank-context rejection, and silent child transport with harness tests.

## 3. Engine operational context

- [x] 3.1 Add a bounded display-safe operational projection for Session status, visible assistant text, current tool label, input waits, and visible errors.
- [x] 3.2 Prove the projection excludes reasoning, raw tool arguments/results, and protected Chat parts.
- [x] 3.3 Add a latest-value per-call context channel and observe the originating Chat without reading the raw Run Journal.

## 4. Active Session lifecycle and steering

- [x] 4.1 Allow Live start during `Working` and `AwaitingInput` only when OMP advertises session-context support; preserve idle compatibility with older OMP.
- [x] 4.2 Prove starting Live during active work does not interrupt, replace, or duplicate the run and that silence creates no durable entry.
- [x] 4.3 Route confirmed Live delegations through `SessionsEngine::steer`, falling back to exactly one ordinary durable turn when no live steerable run remains.
- [x] 4.4 Preserve delegation progress/final speech, navigation continuity, and parked-run restart behavior.

## 5. Contracts and validation

- [x] 5.1 Update the nearest DOX owners and remove stale active-run unavailability text.
- [x] 5.2 Run OMP focused checks plus Comet format, harness, engine, UI, build, and strict OpenSpec gates.
- [ ] 5.3 Smoke the signed app during a real streaming OMP run, including silence, status question, rejected instruction, confirmed steer, and run continuity.
- [ ] 5.4 Archive `fix-live-voice-delegation-continuity` first, archive this change second, and validate all canonical specs.
