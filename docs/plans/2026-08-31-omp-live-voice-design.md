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

Extend the OMP RPC protocol with a headless Live frontend capability. The Live child reuses OMP's Codex transport and media implementation but delegates coding work back to Comet. Comet submits each delegation through its existing durable command queue and ordinary `OmpHarness` run.

```text
Comet gpui
   │ local typed control/events; no audio
   ▼
Comet LiveVoiceRuntime ── OMP Live child
   │                        ├── OAuth + DeviceCheck + signaling
   │                        ├── microphone → Opus/WebRTC
   │                        └── Codex audio → Opus decode → speaker
   │
   └── durable QueueCommand ── OmpHarness run child ── tools/subagents/results
              ▲                                          │
              └──────── existing SessionsEngine folding ─┘
```

The two child processes share one native OMP session identity but retain different write ownership:

- the Live child resumes the Chat's native OMP session before connecting realtime voice;
- for a new Chat, the Live child creates a normal OMP session and Comet records its identity immediately;
- the Live child remains a media and conversational frontend in host-delegation mode: it does not execute tools or persist casual voice turns;
- the run child follows the existing durable Comet path, resumes that same session, and remains the only writer for coding work;
- progress and the final backend result return to the Live child as text-only context append commands.

This gives voice and text one OMP conversation identity while preserving Comet's command ledger, Run Journal, folding, and single-writer execution invariants.

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
{ "id": "live-1", "type": "live_start", "delegationMode": "host" }
{ "id": "live-2", "type": "live_set_muted", "muted": true }
{
  "id": "live-3",
  "type": "live_append_context",
  "delegationId": "del_123",
  "kind": "progress",
  "text": "Inspecting authentication call sites."
}
{
  "id": "live-4",
  "type": "live_append_context",
  "delegationId": "del_123",
  "kind": "final",
  "text": "The authentication regression is fixed and the focused tests pass."
}
{ "id": "live-5", "type": "live_stop" }
```

`live_start` uses OMP's effective `live.voice` setting; Comet does not add a second voice preference in the first release. RPC Live requires `delegationMode: "host"`: OMP emits delegation requests but does not call its internal `AgentSession`.

Commands are correlated through the existing RPC response envelope. `live_append_context` is accepted only for the active delegation. A `final` append closes that delegation and returns Live to listening. `live_stop` is idempotent. A second `live_start` while Live is starting, connected, or stopping fails explicitly.

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

In host-delegation mode, `live_delegation_created` replaces the internal `sendCustomMessage(..., { triggerTurn: true })` call.

Required ordering:

```text
OMP Live: live_delegation_created
Comet:    durable QueueCommand
Comet:    normal OmpHarness AgentEvent stream and folding
Comet:    live_append_context(progress | final)
OMP Live: spoken progress/result
```

OMP permits only one unresolved host delegation. Comet sends context only with the matching `delegationId`; stale or unknown IDs fail explicitly.

### OMP process ownership

Before `live_start`, Comet:

1. launches a normal `omp --mode rpc-ui` child in the Chat Checkout cwd;
2. negotiates RPC protocol v2 and verifies the `liveVoice` ready capability;
3. sends `switch_session` when the Chat already has an OMP session;
4. reads `get_state` and requires a non-empty session identity;
5. issues `live_start` with `delegationMode: "host"`;
6. records the returned session identity on the Chat.

The Live child resumes the Chat's native session but does not register coding host tools, trigger internal `AgentSession` turns, or persist casual voice transcripts. Those mutations remain on the existing per-run `OmpHarness` path after Comet durably queues a delegation.

For a new Chat, the normal RPC child creates the first OMP session; Comet stores that identity before exposing Live as active. The Live child persists between delegations. Each backend run resumes the same identity through Comet's ordinary process lifecycle, so compaction, tools, subagents, questions, model context, and session persistence remain existing behavior.

## Comet harness boundary

Live Voice is an optional harness capability. The shared harness contract exposes capability probing and a Live frontend start operation with an unsupported default; only `OmpHarness` implements it.

The Live frontend stream carries only transient events:

```text
LiveVoiceEvent::Phase
LiveVoiceEvent::Levels
LiveVoiceEvent::Transcript
LiveVoiceEvent::Delegation
LiveVoiceEvent::Ended
```

Its control channel carries mute, stop, and delegation context appends. `OmpNormalizer` and the normal `OmpHarness::run` path remain the sole parser for backend text, reasoning, tools, usage, questions, workflow activity, subagents, and errors. The Live path does not normalize or fold backend agent events.

Terminal backend `AgentEvent::Done` ends only the ordinary run child. The engine sends the final result to the still-connected Live child, which can then accept another delegation.

## Engine lifecycle

The engine owns at most one device-local `LiveVoiceRuntime`:

```text
LiveVoiceRuntime
├── chat_id
├── session-bound OMP Live handle
├── phase
├── active delegation + owned command id
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

1. subscribe to the Chat's existing `SessionsEngine` event hub before dispatch;
2. mint stable command and message IDs from the Live call ID and `delegationId`;
3. queue one ordinary `SessionCommandPayload::Run` with the spoken request;
4. let the existing host executor, `SessionsEngine::dispatch`, Run Journal, document folding, tools, questions, and subagents handle the run unchanged;
5. forward bounded progress text and the final assistant result to the Live child through `live_append_context`;
6. keep the Live child connected for another delegation.

The active runtime records the exact command ID it owns. Before executing any durable command, the host executor asks `SessionsEngine` to stop Live unless the command ID is that active voice delegation. This enforces “normal command stops Live” without adding a new durable command schema or relying on an ID prefix.

Only one voice delegation may be active. Casual user and realtime-assistant transcripts stay only in local Live state. The spoken paraphrase of the backend result is not persisted because the normal assistant result already is.

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
send live_stop
→ OMP stops AudioCapture
→ send session.close
→ close sideband
→ close WebRTC and playback
→ shut down the session-bound Live child
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

The Chat remains visible, and delegated work enters its normal transcript. `Esc` ends Live. Comet's completion/request notification chimes are suppressed while Live owns speaker playback.

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
- Session resume cancellation or missing OMP session identity: fail Live startup before exposing an active call.
- Signaling or sideband failure before delegation: ephemeral Live error only.
- Live child exit during a backend run: stop the Live surface; allow the already-durable normal run to finish and retain its transcript.
- Backend run failure: preserve existing run error folding and send a final failure context to Live when the Live child is still connected.
- Live child exit while idle: close the Live surface with an error; no Chat mutation.
- Unknown additive Live event: bounded diagnostic and ignore; do not crash the process.
- Repeated stop after failure: successful idempotent cleanup.
- No automatic fallback to local dictation or public Realtime API.

## Verification

### OMP protocol tests

- ready frame advertises `liveVoice` only when compiled with the capability;
- commands parse, correlate, and reject invalid state;
- host-delegation start never calls the internal `AgentSession`;
- only one unresolved delegation is permitted;
- progress/final context appends require the active delegation ID;
- a final append returns the controller to listening;
- second start fails;
- stop is idempotent;
- mute forwards to the active controller;
- signaling failure emits one terminal Live event;
- RPC disposal closes capture and playback.

### Comet harness tests

A fake OMP RPC fixture proves:

- capability discovery;
- the Live child switches to an existing Chat session before `live_start`;
- a new Chat returns a session identity that the engine can persist;
- the Live child starts without model setup or host-tool registration;
- transient phase/transcript/delegation parsing;
- progress and final context append encoding;
- two serial delegations reuse one session-bound Live child;
- ordinary backend runs still resume the same OMP session and use the existing normalizer;
- stop and unexpected Live child exit produce deterministic terminal events.

### Engine tests

- casual transcript events do not modify the Chat Transcript;
- one delegation queues exactly one durable Run command and creates one user entry;
- the owned voice command does not stop its Live runtime;
- a different durable command stops Live before execution;
- assistant text, tools, questions, and subagents use the unchanged SessionsEngine folding;
- progress/final text returns through the Live control channel without a second durable assistant entry;
- Live start resumes an existing Chat session or records the new session identity;
- the next normal text run resumes the same identity created or selected by Live;
- backend run failure remains durable even if the Live child exits;
- remote Chat, non-OMP Chat, unsupported OMP, active backend run, and concurrent Live are rejected;
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
- Voice and text use the same native OMP session identity for the Chat.
- Delegated work appears exactly once in the durable Chat Transcript.
- Backend tools and final output retain existing Comet behavior.
- The spoken result does not duplicate the durable assistant response.
- No audio crosses Comet RPC, CRDT, Edge, DeviceRoom, or uploads.
- Stop and every terminal failure release microphone and playback.
- Other harnesses and remote Chat behavior remain unchanged.
