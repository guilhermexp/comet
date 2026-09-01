# Design: Local OMP Live Voice

## Reference

The full architecture and lifecycle analysis is recorded in [`docs/plans/2026-08-31-omp-live-voice-design.md`](../../../docs/plans/2026-08-31-omp-live-voice-design.md). This delta fixes the implementation boundary and invariants that the change must preserve.

## Architecture

Comet uses two OMP child processes with distinct ownership:

```text
gpui Live surface
      │ local typed controls/events; no audio
      ▼
SessionsEngine ───── ephemeral OMP Live frontend child
      │                         │
      │ durable Run command     │ delegation/progress/final text
      ▼                         │
command ledger ── OmpHarness backend run child
      │                         │
      └── existing AgentEvent folding ───────────────┘
```

The Live child is an ephemeral voice frontend. It handles authentication, DeviceCheck, signaling, microphone capture, WebRTC/Opus, and playback. It does not resume or mutate the Chat's native OMP session and does not execute delegated coding work through its own `AgentSession`.

The backend child is the existing OmpHarness run. It resumes the Chat session, handles tools and subagents, emits normalized `AgentEvent` values, and remains the only source of durable Chat Transcript and Run Journal state.

## Decisions

- **D-01: Two OMP processes.** The Live frontend stays connected across delegations while each delegation uses an ordinary backend run child. Sharing one process would bypass or duplicate the durable run lifecycle.
- **D-02: Capability probing.** Availability requires `ready.capabilities.liveVoice === 1`; version-string inference is prohibited.
- **D-03: Durable delegation.** A `live_delegation_created` frame becomes one `SessionCommandPayload::Run` through the existing command ledger. The runtime stores the exact owned command ID for preemption decisions.
- **D-04: Existing folding remains authoritative.** The Live path never normalizes backend tools, text, usage, questions, or completion. Backend progress and final text are observed from the normal run and appended to Live as bounded transient context.
- **D-05: Device-local transient state.** Phase, levels, captions, Live errors, child handles, and delegation correlation live only in the host engine. They are never added to CRDT documents, DeviceRoom frames, uploads, or logs.
- **D-06: Local-only RPC.** Start, mute, stop, and state observation are engine-local methods and are excluded from the forwardable RPC set.
- **D-07: One call and one unresolved delegation.** One Live runtime may exist per device. It owns at most one delegation and accepts context only for that delegation ID.
- **D-08: Deterministic release.** End, Escape, Chat switch, surface close, engine shutdown, transport failure, app quit, or any competing durable command stops Live before conflicting work proceeds.
- **D-09: OMP owns media.** No audio frame crosses Comet's harness boundary; Comet adds no audio dependency or public Realtime API integration.

## Availability

Start is accepted only when the selected Chat is hosted locally, uses OMP, is not archived, has no active backend run, no other Live call exists, and the installed OMP advertises the capability. The engine enforces these conditions even if a caller bypasses the UI.

## Delegation flow

1. OMP Live emits a correlated delegation request.
2. SessionsEngine queues one durable Run command and records its exact command ID in the active runtime.
3. The existing host executor and OmpHarness backend path execute that command.
4. Bounded backend commentary is appended as `progress` context.
5. The terminal backend answer is appended as `final` context, releasing the Live delegation while preserving the call.
6. A different durable command ID preempts and stops Live before execution.

## Packaging

The signed macOS app declares `NSMicrophoneUsageDescription`. The final smoke must launch the packaged app through Finder so macOS attributes permission to the actual app bundle rather than a terminal process.
