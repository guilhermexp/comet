# Live Voice During an Active Run — Design

## Goal

Allow the user to start Live Voice while the selected Chat's device-local Session is already `Working` or `AwaitingInput`, ask about the ongoing run by voice, and optionally send a confirmed instruction into that run.

The existing conversation behavior remains unchanged: activating Live does not make it speak, progress updates do not create speech, and casual voice exchanges remain transient.

## Product behavior

Live Voice is available for an otherwise eligible local OMP Chat when its Session is `Idle`, `Working`, or `AwaitingInput`.

```text
Session Working
      │
      ├── start Live ──> Live Listening
      │                       │
      │                       ├── silence ──> no action
      │                       ├── question ──> transient voice answer
      │                       └── possible instruction
      │                                  │
      │                                  ├── Live asks for confirmation
      │                                  ├── rejected ──> no action
      │                                  └── confirmed ──> steer active run
      │
      └── run completes ──> Live remains available
```

Invariants:

- Starting Live does not interrupt, restart, or duplicate the active run.
- Receiving operational context never causes proactive speech.
- Casual questions and answers do not enter the Chat Transcript.
- No instruction reaches the coding agent without explicit voice confirmation.
- A confirmed instruction produces at most one durable user entry and one execution path.

## Decision

Add a silent, bounded operational-context stream from the host engine to the OMP Live frontend.

When Live starts during an active run, the engine sends an initial snapshot from the current Session and visible Chat state, then observes subsequent `AgentEvent`s for the originating Chat. It coalesces those events into the latest operational projection and sends updates without triggering speech or a coding turn.

The projection contains only information needed to answer status questions:

- Session status;
- recent visible assistant text;
- current visible action or tool label;
- a visible input wait;
- a visible error.

It excludes audio, casual Live transcripts, reasoning deltas, raw Run Journal data, and protected tool payloads or results.

## OMP Live protocol

The existing `live_append_context` command is scoped to a Live-created delegation and cannot describe a run that was already active when Live started. Add an independent operational-context control to the OMP Live protocol.

The control has latest-value semantics. Receiving it updates the Live model's knowledge of the current run but never initiates speech, emits a delegation, or mutates the Chat.

OMP Live retains ownership of the transient voice conversation and confirmation exchange. It emits a delegation only after the user explicitly confirms that a proposed instruction should be sent to the coding agent.

## Delegation routing

A confirmed Live delegation uses the current Session state:

- `Working`: route through the existing `SessionsEngine::steer` mailbox.
- `Idle` or a run that settles during confirmation: queue one ordinary durable run through the existing host executor.
- `AwaitingInput`: report the wait through voice, but do not answer structured authorization or input forms automatically; those remain owned by the existing UI.

Voice-originated steering must be recognized as belonging to the active Live call so the competing-command gate does not stop its own call. Existing unrelated text commands retain the current behavior and stop Live before execution.

## Lifecycle and navigation

Live remains tied to its originating Chat. Selecting another Chat or surface does not move the operational observer or change which run the voice agent describes.

When the observed run completes, the operational context transitions to `Idle` or `Errored`; the Live call remains active. Explicit End, applicable Escape handling, app shutdown, transport failure, and unrelated durable commands retain their existing lifecycle behavior.

## Backpressure and failure semantics

Operational context must never backpressure the coding run. The engine coalesces pending updates and replaces stale snapshots with the latest state.

If operational context becomes unavailable, Live must state that it cannot observe the current run rather than infer progress. A context-channel failure must not alter or cancel the coding run.

If the run settles between confirmation and steering, the confirmed instruction falls back to one normal durable turn. It must never be both steered and dispatched.

## Alternatives rejected

### Remove only the active-run precondition

This enables the microphone but leaves Live unaware of a run that began before the call. It can answer from stale session context and does not satisfy the status-question behavior.

### Snapshot only at Live start

A one-time snapshot becomes stale as streaming continues and cannot answer later questions reliably.

### Query tool inside the Live model

A dedicated status tool would be precise, but adds a second request/classification path when the engine already owns the event stream. The silent operational projection is smaller and preserves the existing conversation flow.

## Verification

Behavioral coverage must prove:

- Live starts while the Session is `Working` and `AwaitingInput`.
- Starting Live does not interrupt or replace the active run.
- Silence produces no speech, delegation, or durable message.
- A status question is answered from current operational context without changing the run.
- A possible instruction produces a transient confirmation and no durable message before acceptance.
- Confirmation during `Working` produces exactly one `steer`.
- Rejection produces no Chat or Session mutation.
- Concurrent run completion preserves the confirmed instruction as exactly one new durable turn.
- Context coalescing cannot block the run.
- A real app smoke starts Live during streaming, asks for status, confirms a change, and observes continuity of the same Session.
