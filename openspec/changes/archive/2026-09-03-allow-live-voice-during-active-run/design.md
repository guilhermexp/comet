## Context

The host engine already owns the Live child, the Chat's device-local Session status, a broadcast stream of normalized `AgentEvent`s, and a durable steering mailbox. Live start currently rejects `Working` and `AwaitingInput`. The existing backend observer forwards visible progress only for a delegation created by that Live call, so merely removing the availability gate would leave a call started mid-run unaware of the current execution.

OMP already has the underlying Frameless Bidi `session.context.append` primitive, but its RPC Live surface exposes only delegation-scoped context. OMP's ready frame advertises basic Live Voice as numeric capability version 1.

## Goals / Non-Goals

**Goals:**

- Permit Live start during active work only when OMP can receive silent session context.
- Keep one coding run and one transient voice call attached to the same Chat.
- Provide bounded display-safe progress without reading raw Run Journal payloads.
- Preserve exactly-once durable routing for confirmed voice instructions.
- Keep older OMP versions working for idle Live calls.

**Non-Goals:**

- Make Live speak proactively about progress, errors, or completion.
- Persist casual voice questions, answers, or operational context.
- Let voice answer structured input or authorization requests automatically.
- Expose reasoning deltas, raw tool arguments/results, or audio through Comet.
- Add a second coding runtime or a second device-local Live call.

## Decisions

### D1: Add an independent additive OMP capability

OMP keeps `liveVoice: 1` and additionally advertises `liveVoiceSessionContext: 1`. Comet probes both values in one temporary OMP process. Basic support remains sufficient while the Session is `Idle`; active work requires both.

This avoids bumping `liveVoice` and accidentally making a newer OMP look unsupported to older Comet builds.

### D2: Expose the existing session-context primitive through RPC

OMP adds `live_append_session_context` and forwards validated, chunked text through `buildSessionContextAppend(text, "commentary")`. Commentary context updates model knowledge but do not request a response, create audio, or emit a delegation.

The OMP Live instructions state that operational context is silent, status questions are conversational, and a change/add-work request during active work requires explicit confirmation before host delegation. The confirmation exchange remains inside the transient realtime conversation; Comet receives nothing until OMP emits the confirmed delegation.

### D3: Maintain one latest-value operational projection per call

The engine subscribes to the originating Chat before capturing its initial Session/Chat Transcript snapshot, preventing an attach race. A small accumulator retains only:

- Session status;
- bounded visible assistant text;
- the current display-policy tool label, without its detail/payload;
- input-wait state;
- a visible error.

The coordinator uses a Tokio watch channel, so producers replace stale pending snapshots instead of queueing every stream delta. The existing Live handle task forwards the newest snapshot through `LiveVoiceControl::AppendSessionContext`. The run's broadcast path never awaits this transport.

The initial visible text comes from the Chat Transcript. Future updates come from the live broadcast subscription. The observer does not replay or inspect the raw Run Journal.

### D4: Route confirmed delegation steer-first

`handle_live_delegation` claims the existing Live ownership ids and subscribes to backend output before routing. It first calls `SessionsEngine::steer` with the owned message id. `Accepted` keeps the current run; `NotSteerable` queues the same instruction and message id through the ordinary host executor. Existing ledger and message-id deduplication cover the race where a run dies around mailbox acceptance.

The existing delegation backend observer remains responsible for returning speakable progress/final context. The ambient operational observer is read-only and never completes a delegation.

### D5: Structured input remains UI-owned

A Live call may start while the Session is `AwaitingInput` and may explain that state, but operational context includes no structured question payload and Live does not resolve it. The user continues through the existing UI authorization/input path.

## Risks / Trade-offs

- Explicit confirmation is a realtime model instruction rather than a host-observable protocol state. Unit tests can prove that context alone emits no delegation, but the spoken confirmation behavior requires an actual OMP Live integration smoke.
- Context sent as commentary depends on the provider preserving silent semantics. The OMP protocol integration test and real-app smoke are required before archive.
- A single current tool label compresses parallel tool activity. This is intentional: it avoids retaining payload-bearing event state and is sufficient for a brief voice status answer.
- Context transport failure may end the Live call, but cancellation remains scoped to Live and must not alter the coding run.
- Two active OpenSpec changes modify `omp-live-voice`. Archive `fix-live-voice-delegation-continuity` first, then archive this change so the final canonical requirement includes both navigation continuity and active-run availability.
