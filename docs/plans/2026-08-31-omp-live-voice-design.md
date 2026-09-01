# OMP Live Voice for Comet Desktop — Design

## Goal

Add the OMP Codex-backed realtime voice experience to the native Comet desktop app on macOS without reimplementing Codex Live media transport inside Comet.

The first release is intentionally narrow:

- OMP Chats only;
- Chat hosted on the current Mac only;
- one active Live call per device;
- OMP owns microphone capture, Codex OAuth, DeviceCheck, WebRTC, Opus, and speaker playback;
- Comet owns the desktop controls, transient captions, Chat Transcript, Run Journal, and lifecycle policy;
- only delegated coding work enters the durable Chat Transcript;
- audio and casual voice conversation never enter CRDT state, Edge, DeviceRoom, uploads, or logs.

## Current system constraints

Comet drives OMP through `omp --mode rpc-ui`. `OmpHarness` starts one OMP child per live Comet run, resumes the native OMP session with `switch_session`, normalizes OMP frames into `AgentEvent`, and stops the child after the terminal `agent_end`.

OMP Live currently exists only as an interactive-mode surface:

- `LiveCommandController` owns the TUI presentation;
- `LiveSessionController` coordinates realtime conversation and the normal OMP `AgentSession`;
- `CodexLiveTransport` owns OAuth signaling and the sideband WebSocket;
- `LiveWebRtcPeer` owns WebRTC, Opus media, microphone capture, and playback.

The canonical OMP RPC command set does not expose Live controls or Live events. Sending `/live` as a normal RPC prompt therefore cannot create the interactive Live surface.

## Decision

Extend the OMP RPC protocol with a headless Live capability. The extension reuses `LiveSessionController` and emits its presentation callbacks as additive JSONL events.

```text
Comet gpui
   │ local typed control/events; no audio
   ▼
Comet engine ── OmpHarness ── omp --mode rpc-ui
                                  │
                                  ├── OAuth + DeviceCheck + signaling
                                  ├── microphone → Opus/WebRTC
                                  ├── Codex audio → Opus decode → speaker
                                  └── AgentSession → tools/subagents/results
```

This keeps vendor-specific behavior in `zeron-harness`, keeps durable behavior in `zeron-engine`, and keeps pixels in `zeron-ui`.

## Alternatives rejected

### Reimplement Codex Live in Comet

This would duplicate a private protocol: Codex Desktop headers, ChatGPT OAuth handling, Apple DeviceCheck attestation, `quicksilver` signaling, sideband events, WebRTC, Opus packet-loss handling, barge-in gating, and future protocol changes. It also moves vendor-specific behavior out of the harness boundary.

### Use the public OpenAI Realtime API

This is a different product contract. It requires API-key billing and does not reproduce the ChatGPT/Codex subscription path or `gpt-live-1-codex` delegation behavior.

### Transport PCM through Comet RPC

This would require a binary realtime media protocol over local IPC and DeviceRoom relay, plus jitter, buffering, reconnection, and audio-device ownership in Comet. It is unnecessary for a local-only first release.

## OMP RPC extension

### Capability discovery

The OMP ready frame advertises the additive capability when available:

```json
{
  "type": "ready",
  "protocolVersion": 1,
  "supportedProtocolVersions": [1, 2],
  "capabilities": {
    "liveVoice": 1
  }
}
```

An older OMP omits the field. Comet must not infer support from version strings. When `liveVoice` is absent, the desktop control is unavailable with an actionable update message.

### Commands

```json
{ "id": "live-1", "type": "live_start" }
{ "id": "live-2", "type": "live_set_muted", "muted": true }
{ "id": "live-3", "type": "live_stop" }
```

`live_start` uses OMP's effective `live.voice` setting; Comet does not add a second voice preference in the first release.

Commands are correlated through the existing RPC response envelope. `live_stop` is idempotent. A second `live_start` while Live is starting, connected, or stopping fails explicitly.

### Events

```json
{ "type": "live_phase", "phase": "listening" }
```

```json
{ "type": "live_levels", "input": 0.18, "output": 0.04 }
```

```json
{
  "type": "live_transcript",
  "role": "user",
  "turn": 3,
  "text": "Analyze the authentication module",
  "final": true
}
```

```json
{
  "type": "live_delegation_created",
  "delegationId": "del_123",
  "request": "Analyze the authentication module in the current repository."
}
```

```json
{ "type": "live_ended", "error": null }
```

The phase values are `connecting`, `listening`, `speaking`, `working`, `muted`, and `error`.

Audio never appears in RPC events. `output_audio.delta` remains inside the native OMP media peer.

### Delegation ordering

`live_delegation_created` must be emitted synchronously before `LiveSessionController` calls `sendCustomMessage(..., { triggerTurn: true })`.

Required ordering:

```text
live_delegation_created
agent_start
message/tool events
agent_end
```

This gives Comet a durable user-side boundary before assistant output arrives. The OMP RPC writer must preserve this order. Comet may buffer early agent events defensively, but ordering is an OMP protocol contract rather than a timing assumption.

### OMP session ownership

Before `live_start`, Comet performs the same setup as a normal OMP run:

1. launch `omp --mode rpc-ui` in the Chat Checkout cwd;
2. negotiate RPC protocol v2;
3. enable subagent events;
4. resume the Chat's native `sessionFile` with `switch_session`;
5. register applicable host tools;
6. apply the selected model and thinking level;
7. read state and retain the current `sessionFile`;
8. issue `live_start`.

The same OMP `AgentSession` handles every delegation during the call. Compaction, tools, subagents, questions, model context, and native session persistence remain OMP behavior.

## Comet harness boundary

Live Voice is an optional harness capability. The shared harness contract exposes support and a Live start operation with an unsupported default; only `OmpHarness` implements it.

The Live stream multiplexes transient voice events and the already-normalized agent stream:

```text
LiveVoiceEvent::Phase
LiveVoiceEvent::Levels
LiveVoiceEvent::Transcript
LiveVoiceEvent::Delegation
LiveVoiceEvent::Agent(AgentEvent)
LiveVoiceEvent::Ended
```

`OmpNormalizer` remains the sole parser for text, reasoning, tools, usage, questions, workflow activity, subagents, and errors. The Live path must not duplicate normalization or document folding.

The OMP Live runner differs from a normal OMP run in one lifecycle rule: terminal `agent_end` closes the current delegated run but does not stop the OMP process. Only `live_stop`, transport failure, or host teardown ends the Live process.

## Engine lifecycle

The engine owns at most one device-local `LiveVoiceRuntime`:

```text
LiveVoiceRuntime
├── chat_id
├── OmpProcess / harness handle
├── phase
├── active delegation
├── control sender
└── event task
```

### Preconditions

Live start succeeds only when:

- the Chat is hosted by the local device;
- the Chat's selected harness is OMP;
- the installed OMP advertises `liveVoice`;
- the Chat is not archived or in an incompatible active run;
- no other local Live call exists.

The local-only rule is enforced in the engine, not only by hiding controls in the UI.

### State machine

```text
idle
  → connecting
  → listening ↔ speaking ↔ muted
  → working
  → listening
  → stopping
  → idle
```

Any active state can transition to `error`, followed by the same teardown path used by explicit stop.

### Delegated runs

The Live connection is ephemeral engine state and does not occupy the durable Chat command queue while listening.

For each `live_delegation_created`:

1. start a delegated run segment and Run Journal;
2. append the delegation request as one user entry in the Chat Transcript;
3. feed subsequent `AgentEvent` values into the existing run folding path;
4. settle that run on terminal `agent_end`;
5. keep the Live OMP process connected for another delegation.

A new delegation while work is active becomes a user message in the current run, equivalent to steering. The shared event contract must therefore permit an unwrapped main-Chat `AgentEvent::UserMessage`; today that shape is documented as subagent-only.

Casual user and realtime-assistant transcripts stay only in local Live state. The spoken paraphrase of the backend result is not persisted because the normal assistant result already is.

### Interaction with normal commands

The composer is replaced while Live is active, so the local UI cannot send a competing text command accidentally.

If a normal command arrives through another viewport or an already-queued source, the host engine:

1. stops Live gracefully;
2. waits for microphone and OMP Live teardown;
3. runs the command through the normal durable command path.

An idle voice call must never starve the command queue.

### Shutdown triggers

Live stops on:

- explicit End;
- Chat or profile switch;
- window/surface close;
- local UI disconnect;
- sign-out;
- incoming normal command;
- engine shutdown;
- OMP process exit;
- WebRTC or sideband failure;
- system suspend/resume when the transport does not recover.

Teardown order:

```text
stop AudioCapture
→ send session.close
→ close sideband
→ close WebRTC and playback
→ retain updated session identity
→ shut down OMP child
→ clear runtime
→ notify UI
```

Stop is idempotent. Graceful shutdown uses a bounded deadline and then the existing OMP child termination escalation.

## RPC and sync boundary

Comet adds local typed operations for:

- start Live for a Chat;
- set mute;
- stop Live;
- watch the current local Live state.

These operations are IPC-local and are never relay-forwardable. The engine rejects a request targeting a remote-hosted Chat even if the current viewport can otherwise control that Chat.

Voice state is not written to:

- the workspace registry CRDT;
- Chat documents, except delegated work;
- DeviceRoom presence;
- Edge;
- uploads;
- Chat Transcript Export.

## Desktop UI

A microphone control appears in the composer only for a supported local OMP Chat. Unsupported states expose a specific disabled reason: remote Chat, non-OMP harness, active run, older OMP, or another active Live call.

Starting Live replaces the editor with one focused native strip containing:

- phase label;
- microphone level visualization;
- latest incremental caption;
- Mute/Unmute;
- End.

The Chat remains visible, and delegated work enters its normal transcript. `Esc` ends Live. Comet's ordinary TTS vocalizer is suspended while Live owns speaker playback.

The UI stores no lifecycle authority. Closing or dropping the surface sends stop, while the engine also owns cleanup if the UI disappears unexpectedly.

## macOS permission and packaging

The packaged app declares:

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Comet uses the microphone for realtime voice conversations with your coding agent.</string>
```

The release must be tested from a signed `.app` launched through Finder. The external `omp` child owns the actual audio device, so the test must verify how macOS TCC attributes the child capture to the responsible GUI process.

If the signed packaged smoke proves that TCC cannot authorize this child topology, the design must be revisited to move capture into Comet and stream PCM to OMP. That fallback is not implemented speculatively.

No audio is recorded to disk. Diagnostics must not log OAuth tokens, attestation payloads, PCM, or casual transcripts.

## Error semantics

- Missing or expired Codex OAuth: actionable error directing the user to OMP/Codex account login.
- Microphone permission denied: explicit permission error; no Chat entry.
- Signaling or sideband failure before delegation: ephemeral Live error only.
- OMP exit during delegation: settle the current run as errored and retain already-folded transcript data.
- OMP exit while idle: close the Live surface with an error; no Chat mutation.
- Unknown additive Live event: bounded diagnostic and ignore; do not crash the process.
- Repeated stop after failure: successful idempotent cleanup.
- No automatic fallback to local dictation or public Realtime API.

## Verification

### OMP protocol tests

- ready frame advertises `liveVoice` only when compiled with the capability;
- commands parse, correlate, and reject invalid state;
- second start fails;
- stop is idempotent;
- mute forwards to the active controller;
- delegation event precedes agent lifecycle events;
- multiple terminal `agent_end` events do not close Live;
- signaling failure emits one terminal Live event;
- RPC disposal closes capture and playback.

### Comet harness tests

A fake OMP RPC fixture proves:

- capability discovery;
- start/resume/model setup ordering;
- transient phase/transcript parsing;
- delegation to normalized agent stream;
- two delegations reuse one child and one native session;
- terminal agent events settle delegations without ending Live;
- stop and unexpected child exit produce deterministic terminal events.

### Engine tests

- casual transcript events do not modify the Chat Transcript;
- one delegation creates exactly one user entry;
- assistant text, tools, questions, and subagents use existing folding;
- a second delegation during work creates a steering-style user boundary;
- normal command arrival stops Live before execution;
- remote Chat, non-OMP Chat, unsupported OMP, and concurrent Live are rejected;
- failure during delegation settles the run as errored;
- UI disconnect and engine shutdown release the runtime.

### UI tests and visual validation

- availability and disabled-reason derivation are pure unit tests;
- state transitions preserve one stable Live strip;
- Mute, End, and Escape emit the expected controls;
- `scripts/dev-demo.sh` validates gpui layout and focus behavior;
- accessibility exposes button roles, labels, mute value, and phase status.

### Packaged macOS smoke

From the signed Finder-launched app:

1. start Live and accept microphone permission;
2. converse without delegation and confirm no Chat mutation;
3. request a real repository operation;
4. observe one user request, tools, and final result in the Chat;
5. hear the result through Codex Live;
6. interrupt output, mute, and unmute;
7. switch Chats and verify the microphone indicator disappears;
8. resume the original OMP Chat and verify native session continuity;
9. quit the app and verify no OMP child retains the microphone.

## Acceptance criteria

- Live is offered only for a supported local OMP Chat.
- Comet reuses OMP's Codex OAuth and Live media implementation.
- Casual voice conversation remains transient.
- Delegated work appears exactly once in the durable Chat Transcript.
- Backend tools and final output retain existing Comet behavior.
- The spoken result does not duplicate the durable assistant response.
- No audio crosses Comet RPC, CRDT, Edge, DeviceRoom, or uploads.
- Stop and every terminal failure release microphone and playback.
- Other harnesses and remote Chat behavior remain unchanged.
