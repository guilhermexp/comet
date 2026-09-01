# OMP Live Voice in Comet

## Status

Implemented and manually accepted on September 1, 2026.

This guide documents the production behavior delivered by Comet commit `91967774` and the compatible OMP RPC implementation inspected at OMP commit `4d7a19cc2dd35c4f53027b060d97ced9ecd58d1b`.

The historical architecture decision remains in [`plans/2026-08-31-omp-live-voice-design.md`](plans/2026-08-31-omp-live-voice-design.md). This document describes the implementation that now exists: ownership, wire contracts, source locations, voice-first Chat creation, session continuity, delegation, cleanup, packaging, verification, and operations.

## What the feature does

A local Comet Chat configured to use OMP can open OMP's Codex-backed realtime voice experience inside the native desktop composer.

Two entry paths are supported:

1. **Existing Chat:** Comet resumes the Chat's stored OMP session and starts Live in the same Checkout.
2. **New Chat, voice first:** the microphone is available before any text is sent. Clicking it materializes a normal Chat from the current device, project, Checkout, model, and OMP choices; starts Live; records the OMP session identity; and selects the Chat only after startup succeeds.

The result matches the user-visible semantics of launching a fresh OMP session and entering `/live` immediately. A later text command uses the same Chat and resumes the OMP session that Live created.

## Non-negotiable invariants

The implementation is organized around these invariants:

- OMP owns Codex authentication, DeviceCheck, signaling, WebRTC, Opus, microphone capture, and speaker playback.
- Comet never transports audio frames.
- Casual voice conversation and incremental captions are transient device-local state.
- Only delegated coding work enters the durable Chat command path.
- The normal backend harness remains the only writer for coding tools, questions, subagents, usage, transcript entries, and terminal answers.
- Voice and text share one native OMP session identity for a Chat.
- One Live call may exist per device.
- Live operations are local-only RPC methods and cannot be relay-forwarded to a remote Chat.
- A competing durable command stops Live before execution unless it is the exact command owned by the active Live delegation.
- Failure cleanup must never delete a Chat or worktree after the user has started using it.

## End-to-end architecture

```text
Native Comet composer
        │
        │ Start / Mute / End / transient state
        ▼
Comet SessionsEngine ────────────── OMP Live frontend child
        │                                  │
        │                                  ├─ Codex OAuth + DeviceCheck
        │                                  ├─ signaling + sideband
        │                                  ├─ microphone + Opus/WebRTC
        │                                  └─ playback + realtime speech
        │
        │ live_delegation_created
        ▼
Durable QueueCommand ledger
        │
        ▼
Normal OmpHarness backend child
        │
        ├─ tools
        ├─ questions
        ├─ subagents
        ├─ usage
        └─ final answer
        │
        └──────── bounded progress/final text ────────► OMP Live child
```

The Live frontend child and the ordinary backend run child are separate processes. They share the same OMP session identity but have separate responsibilities:

- **Live frontend child:** realtime media and conversational frontend; transient in host-delegation mode.
- **Backend run child:** durable coding execution through the existing Comet command ledger and transcript folding.

This separation prevents casual speech from becoming durable coding history while preserving the context needed for voice and text to feel like one conversation.

## Repository and source map

### Shared Comet protocol

| Responsibility | Source |
| --- | --- |
| Live phase, role, transcript, availability, and unavailable reasons | [`crates/proto/src/live_voice.rs`](../crates/proto/src/live_voice.rs) |
| Local RPC method names | [`crates/rpc/src/lib.rs`](../crates/rpc/src/lib.rs) |

The shared protocol defines:

- `LiveVoicePhase`
- `LiveVoiceRole`
- `LiveVoiceUnavailableReason`
- `LiveVoiceTranscript`
- `LiveVoiceState`
- `LiveVoiceAvailability`

These values are vendor-neutral. Codex-specific media details stay inside OMP.

### Harness boundary

| Responsibility | Source |
| --- | --- |
| Optional Live harness contract and unsupported defaults | [`crates/harness/src/lib.rs`](../crates/harness/src/lib.rs) |
| OMP capability probe and frontend startup | [`crates/harness/src/omp/mod.rs`](../crates/harness/src/omp/mod.rs) |
| OMP child process and session switching | [`crates/harness/src/omp/process.rs`](../crates/harness/src/omp/process.rs) |
| OMP Live wire parsing | [`crates/harness/src/omp/protocol.rs`](../crates/harness/src/omp/protocol.rs) |

The harness API carries no audio. Its Live-specific contract is:

```rust
LiveVoiceRequest {
    cwd,
    resume,
}

LiveVoiceHandle {
    session_id,
    events,
    controls,
}
```

The event stream contains phase, levels, transient transcript, delegation, and terminal events. The control channel contains mute, stop, and correlated progress/final context appends.

Unsupported harnesses inherit the default implementation: probe returns `false`, and start returns an unsupported error. OMP is the only production harness that implements the feature.

### Engine ownership

| Responsibility | Source |
| --- | --- |
| Runtime state, delegation ownership, event observation, and cleanup | [`crates/engine/src/live_voice.rs`](../crates/engine/src/live_voice.rs) |
| Preconditions, start, session identity persistence, mute, stop | [`crates/engine/src/sessions.rs`](../crates/engine/src/sessions.rs) |
| Local-only RPC dispatch | [`crates/engine/src/rpc.rs`](../crates/engine/src/rpc.rs) |
| Command execution and competing-command preemption | [`crates/engine/src/doc_host.rs`](../crates/engine/src/doc_host.rs) |

The engine owns one optional device-local runtime. It records:

- owning Chat ID;
- call ID;
- current phase and levels;
- session-bound OMP Live handle;
- current delegation ID;
- exact durable command ID owned by that delegation;
- control sender and observer task.

The exact command ID matters. Prefixes or heuristics cannot safely distinguish the voice-owned run from another queued command.

### Native desktop UI

| Responsibility | Source |
| --- | --- |
| View model, captions, status copy, unavailable reasons | [`crates/ui/src/live_voice.rs`](../crates/ui/src/live_voice.rs) |
| Microphone action, new-Chat materialization, Checkout handling, rollback | [`crates/ui/src/composer.rs`](../crates/ui/src/composer.rs) |
| Availability probe, state watcher, selected-Chat lifecycle | [`crates/ui/src/state.rs`](../crates/ui/src/state.rs) |
| Chat-switch and window-level release behavior | [`crates/ui/src/shell.rs`](../crates/ui/src/shell.rs) |
| Completion-sound suppression while Live owns playback | [`crates/ui/src/sound.rs`](../crates/ui/src/sound.rs) |

### OMP implementation

The compatible OMP checkout implements the RPC extension in:

```text
packages/coding-agent/src/live/controller.ts
packages/coding-agent/src/modes/rpc/rpc-types.ts
packages/coding-agent/src/modes/rpc/rpc-live.ts
packages/coding-agent/src/modes/rpc/rpc-mode.ts
packages/coding-agent/test/rpc-live.test.ts
```

`controller.ts` supports two delegation modes:

- `session`: interactive OMP owns the normal `AgentSession` turn.
- `host`: OMP emits a delegation event and waits for the host to return progress/final context.

Comet always requests `host` mode.

## OMP RPC extension

### Capability

OMP advertises support in the ready frame:

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

Comet checks the capability directly. It does not infer support from an OMP version number.

### Commands

```json
{ "id": "live-1", "type": "live_start", "delegationMode": "host" }
{ "id": "live-2", "type": "live_set_muted", "muted": true }
{
  "id": "live-3",
  "type": "live_append_context",
  "delegationId": "del-123",
  "kind": "progress",
  "text": "Inspecting the affected call sites."
}
{
  "id": "live-4",
  "type": "live_append_context",
  "delegationId": "del-123",
  "kind": "final",
  "text": "The change is complete and the focused tests pass."
}
{ "id": "live-5", "type": "live_stop" }
```

Rules enforced by OMP:

- `live_start` requires `delegationMode: "host"` in RPC mode.
- Start requires an idle OMP session.
- Only one unresolved host delegation may exist.
- Context appends require the active delegation ID.
- `final` resolves the delegation and returns the call to listening.
- `live_stop` is idempotent.

### Events

```json
{ "type": "live_phase", "phase": "listening" }
{ "type": "live_levels", "input": 0.18, "output": 0.04 }
{
  "type": "live_transcript",
  "role": "user",
  "turn": 3,
  "text": "Inspect the authentication module",
  "final": true
}
{
  "type": "live_delegation_created",
  "delegationId": "del-123",
  "request": "Inspect the authentication module in the current repository."
}
{ "type": "live_ended", "error": null }
```

There is intentionally no audio event.

## Eligibility

### Existing Chat

The engine accepts Live only when all conditions hold:

1. The Chat is hosted on the current device.
2. The Chat harness is OMP.
3. The Chat is not archived.
4. No incompatible backend run is active.
5. No other device-local Live call exists.
6. OMP advertises `liveVoice`.

The engine is authoritative. Hiding or disabling a UI control is not a security or lifecycle boundary.

### New-Chat draft

Before a Chat row exists, the composer can safely decide only the draft-local conditions:

- engine connected;
- target device is the current device;
- resolved harness is OMP.

The microphone is shown for that eligible draft. Capability probing and all other authoritative checks occur after the normal Chat has been materialized.

If another harness is selected, or the target is remote, the microphone remains absent by design.

## Voice-first new Chat lifecycle

This is the path added after the initial Live implementation.

### 1. Derive the draft action

`Composer::live_voice_model` resolves the draft harness only when no Chat is selected. It synthesizes an available draft result for a local OMP target and feeds that result through the same `LiveVoiceViewModel` used by existing Chats.

While startup is in flight, the button remains visible but disabled with `Starting Live Voice…`. Repeated clicks cannot create duplicate Chats.

### 2. Snapshot the normal Chat inputs

`Composer::start_live_voice` captures the same choices used by first-text send:

- effective target device;
- selected project/Space;
- Checkout plan;
- resolved OMP harness/model/reasoning/options;
- `ChatConfig`;
- generated Chat ID.

No separate voice-only Chat schema exists.

### 3. Resolve the Checkout

The composer handles all existing Checkout modes:

- **Current checkout:** use the selected project's path and branch.
- **Existing worktree:** use that worktree path and branch.
- **New worktree:** call `CreateWorktree`, retain the returned repository/path identity for rollback, and use the generated worktree path and branch.
- **No project:** run from `~`, matching the normal new-Chat fallback.

The client waits 150 seconds for `CreateWorktree`, above the engine's 120-second forwarding deadline. It captures raw `repoPath` and `path` before strict response deserialization so cleanup remains possible if a response shape is malformed.

### 4. Materialize the ordinary Chat

The shared `create_chat_mutation` builder emits the normal workspace mutation:

```json
{
  "op": "createChat",
  "chatId": "generated-id",
  "spaceId": "space-id",
  "cwd": "/resolved/checkout",
  "branch": "resolved-branch",
  "config": {
    "harness": "omp",
    "model": "resolved-model",
    "reasoning": "resolved-reasoning",
    "modelOptions": {},
    "sandbox": "workspace-write"
  }
}
```

Project-less Chats use `deviceId` instead of `spaceId`. The two ownership fields are mutually exclusive.

The text-send path uses the same builder. Extracting it prevents voice-first and text-first Chat metadata from drifting.

### 5. Start Live before navigating

After `createChat` succeeds, the composer calls the existing local `StartLiveVoice` RPC with the generated Chat ID.

The composer deliberately remains on the draft canvas until the engine confirms startup. This prevents the user from sending a normal command into a Chat that might otherwise be rolled back after a slow or failed Live start.

The START call uses the same RPC lifetime as the existing selected-Chat flow. Engine-side OMP startup and request timeouts bound the normal case. A shorter UI timeout was rejected because a client timeout cannot prove that the server did not start Live.

### 6. Select only after success

After START succeeds, the composer selects the generated Chat only if the user is still on the new-Chat canvas.

If the user navigated elsewhere while worktree, Chat, or Live startup was running:

1. the new selection is preserved;
2. the newly started Live call is stopped because this attempt is known to own it;
3. the untouched generated Chat is deleted;
4. any generated worktree is deleted.

The flow never steals selection from another Chat.

### 7. Preserve the call during its own selection

Live publishes `Connecting` before `StartLiveVoice` returns. Normal Chat switching stops an active Live call, so selecting the newly created owner Chat requires one exception:

```text
stop Live when active_live.chat_id != destination_chat_id
keep Live when active_live.chat_id == destination_chat_id
```

`AppState::live_voice_stops_for_chat_selection` enforces this rule. Selecting another Chat, returning to the new-Chat canvas, or closing the surface still stops Live.

### 8. Roll back only untouched resources

Failure cleanup runs while the generated Chat has never been selected or exposed for normal commands. Therefore it is safe to remove the empty Chat and generated worktree.

Cleanup order:

```text
if this attempt definitely started Live:
    StopLiveVoice
DeleteChat
if this attempt created a worktree:
    DeleteWorktree
clear pending UI state
show actionable failure on the new-Chat draft
```

`StopLiveVoice` is device-global, so it is issued only after this attempt returned a successful START. A failure before START, including `AnotherLiveCall`, must not stop a foreign call.

## Existing Chat and session continuity

For an existing Chat, `AppState::start_live_voice` calls the local START method directly.

The engine obtains the Chat Checkout cwd and stored OMP session identity. The harness then:

1. starts a normal non-ephemeral `omp --mode rpc-ui` child in that cwd;
2. validates the ready capability;
3. sends `switch_session` when `resume` exists;
4. calls `get_state`;
5. requires a non-empty effective session path;
6. sends `live_start` in host-delegation mode;
7. returns the effective session identity in `LiveVoiceHandle.session_id`.

When the Chat has no stored OMP session, the Live child creates a normal one. The engine persists that identity before exposing Live as active.

Every later ordinary text run receives the same identity through the existing `resume_for` and `remember_harness_session` path. Voice-first, text-first, and delegated runs therefore converge on one native OMP conversation.

## Delegated coding work

Casual speech remains inside OMP. Coding work becomes durable only when OMP emits `live_delegation_created`.

The engine then:

1. validates that no delegation is already unresolved;
2. derives stable command/message identities from the call and delegation;
3. subscribes to normal session events before dispatch;
4. queues one ordinary `SessionCommandPayload::Run`;
5. records the exact command ID as Live-owned;
6. lets the existing host executor and `OmpHarness` run the request;
7. forwards bounded backend progress as `live_append_context(progress)`;
8. forwards the terminal result as `live_append_context(final)`;
9. releases delegation ownership while leaving Live connected.

The durable transcript contains one user request and the normal backend result. It does not contain the user's casual voice turns, OMP's realtime paraphrases, or duplicate assistant output.

## Competing commands and lifecycle

A durable command with a different command ID preempts Live before execution. This includes commands from another viewport or a command already queued before the local UI changed surfaces.

The exact Live-owned command is exempt so its backend run can execute while Live remains connected.

Live teardown is triggered by:

- End;
- Escape;
- switching to a different Chat;
- returning to the new-Chat canvas;
- surface/window close;
- app quit;
- engine shutdown;
- OMP process exit;
- transport failure;
- a competing durable command.

Stop is idempotent. Setup failures shut down the OMP child. A closed control channel during unrelated command teardown is best-effort and cannot reject the durable command.

## Desktop state and accessibility

### Idle eligible state

The compact and expanded composer layouts include the same stable microphone action. The action has:

- button role;
- `Start Live Voice` accessible label/tooltip;
- keyboard focus support;
- disabled styling for unavailable or pending states.

### Active state

The editor is replaced by a Live strip showing:

- phase;
- latest bounded caption;
- user/assistant role;
- input and output levels;
- mute state;
- Mute/Unmute;
- End.

Phase labels are stable: Connecting, Listening, Speaking, Working, Muted, Ending, and actionable Error text.

### Audio coexistence

Normal completion/request chimes are suppressed only while Live owns playback. This prevents Comet sounds from colliding with Codex Live speech.

## Data durability matrix

| Data | Durable? | Owner |
| --- | --- | --- |
| Raw microphone audio | No | OMP media peer |
| Decoded playback audio | No | OMP media peer |
| Casual user transcript | No | Local Live runtime/UI |
| Casual assistant transcript | No | Local Live runtime/UI |
| Live phase and levels | No | Local Live runtime/UI |
| Delegated coding request | Yes, once | Comet command ledger/Chat document |
| Backend tools and questions | Yes | Existing SessionsEngine folding |
| Backend final answer | Yes | Existing SessionsEngine folding |
| OMP session identity | Yes | Chat harness-session metadata |
| Live call/delegation ownership | No | Device-local engine runtime |

No audio or casual transcript is written to CRDT, DeviceRoom, Edge, uploads, run logs, or Chat export.

## Local RPC boundary

Comet exposes these engine-local methods:

```text
ProbeLiveVoice
StartLiveVoice
SetLiveVoiceMuted
StopLiveVoice
WatchLiveVoice
```

`crates/engine/src/rpc.rs` explicitly excludes them from the forwardable method set. A remote viewport cannot make the current device forward Live control to another device.

## Error behavior

| Failure | Behavior |
| --- | --- |
| Remote Chat | Reject with host-device guidance |
| Non-OMP Chat/draft | Hide or reject with OMP-only guidance |
| Archived Chat | Reject with unarchive guidance |
| Active incompatible run | Reject until the run stops |
| Unsupported OMP | Reject with update guidance |
| Another Live call | Reject without stopping the existing call |
| Missing session identity | Fail startup and shut down the child |
| Codex authentication failure | Surface OMP/Codex login error |
| Microphone permission denial | Surface permission error; no durable voice transcript |
| Signaling/WebRTC failure | End transient Live state; durable delegated run may continue |
| Backend run failure | Preserve normal durable error folding and return final failure context when possible |
| Closed Live control channel during another command | Continue the durable command; cleanup is best-effort |
| Repeated stop | Succeed without duplicate terminal state |

## macOS microphone permission and packaging

The app bundle declares:

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Comet uses the microphone for realtime voice conversations with your coding agent.</string>
```

Source: [`dist/macos/Info.plist`](../dist/macos/Info.plist).

Build and sign the package:

```bash
CODESIGN_IDENTITY="Apple Development: …" scripts/package-macos.sh
codesign --verify --deep --strict --verbose=2 target/package/Zeron.app
```

Artifacts:

```text
target/package/Zeron.app
target/package/zeron-0.2.18-macos-arm64-app.tar.gz
target/package/zeron-0.2.18-macos-arm64.dmg
```

During sibling OMP development, a Finder-launched app must inherit the Live-capable OMP override through `launchctl`:

```bash
launchctl setenv OMP_EXECUTABLE "$PWD/scripts/omp-dev"
open -na "$PWD/target/package/Zeron.app"

# After testing
launchctl unsetenv OMP_EXECUTABLE
```

`OMP_EXECUTABLE` must not remain globally set after the smoke test.

## Verification inventory

### OMP

`packages/coding-agent/test/rpc-live.test.ts` covers:

- capability advertisement;
- command/event wire types;
- host delegation;
- invalid session delegation mode;
- lifecycle start/mute/context/stop;
- transient event emission.

### Comet harness

The fake OMP RPC fixture covers:

- capability parsing;
- existing-session switch before Live start;
- new-session identity return;
- setup failure child shutdown;
- phase/transcript/delegation parsing;
- context append and stop controls;
- normal backend run continuity.

Run:

```bash
cargo test -p zeron-harness omp_live -- --nocapture
```

### Comet engine

Engine tests cover:

- eligibility reasons;
- one-call ownership;
- existing and newly created OMP session identity;
- later text resume;
- one durable command per delegation;
- progress/final context;
- competing command preemption;
- non-Applied command ownership release;
- closed-control-channel command continuity;
- idempotent stop.

Run:

```bash
cargo test -p zeron-engine live_voice -- --nocapture
```

### Comet UI

UI tests cover:

- existing and draft microphone visibility;
- local OMP draft eligibility;
- current checkout, existing worktree, and new worktree planning;
- shared `createChat` mutation shape;
- Live phase/caption/level derivation;
- active-call surface;
- selection of the owning Live Chat without stopping its Connecting call;
- switch-away stop behavior.

Run:

```bash
cargo test -p zeron-ui live_voice -- --nocapture
cargo test -p zeron-ui -- --nocapture
cargo build -p zeron
```

Final verification recorded for the accepted implementation:

- Live-focused UI tests: 16 passed.
- Complete UI suite: 1005 passed.
- OpenSpec strict validation: 29 passed, 0 failed.
- Signed app verification: valid on disk and satisfies its designated requirement.
- Code review: lifecycle rollback, selection, worktree, and foreign-call races resolved; no remaining actionable finding.
- Manual acceptance: voice-first new Chat flow confirmed working by the user on September 1, 2026.

## Manual acceptance procedure

### New Chat, voice first

1. Launch the signed app with the Live-capable OMP executable.
2. Open the new-Chat canvas.
3. Select the local Mac, a project, a Checkout, and OMP.
4. Confirm the microphone appears before any text is sent.
5. Click the microphone.
6. Confirm a normal Chat appears only after Live starts.
7. Speak a request and confirm input level/caption activity.
8. End Live.
9. Send a normal text command.
10. Confirm the text run continues the same OMP conversation.

### Existing Chat continuity

1. Send a unique fact through normal text in an OMP Chat.
2. Start Live.
3. Ask about that fact.
4. Confirm Live has the existing Chat context.
5. End Live and send another text command.
6. Confirm the conversation remains continuous.

### Failure/cleanup checks

1. Select a non-OMP harness and confirm the draft microphone is absent.
2. Start Live in one Chat and verify another Chat cannot start a second call.
3. Switch Chats and confirm Live releases microphone/playback.
4. Start voice-first with a fresh worktree, navigate away during startup, and confirm selection is not stolen and the untouched worktree is removed.
5. Quit the app and confirm no OMP child retains the microphone.

## Troubleshooting

### Microphone is missing on a new Chat

Check, in order:

1. The harness selector is OMP. The model label may show a provider/model such as `openai-codex/...` or `google-antigravity/...`; the harness icon/selection must still be OMP.
2. The selected target is the current Mac, not a remote device.
3. The engine is connected.
4. The app was rebuilt after commit `91967774`.

### Tooltip says OMP must be updated

The executable reached by `OMP_EXECUTABLE` does not advertise `capabilities.liveVoice = 1`. Use the sibling development wrapper or install the compatible OMP build.

### Signed app uses the wrong OMP

Terminal exports are not automatically inherited by Finder-launched apps. Set `OMP_EXECUTABLE` through `launchctl`, launch a fresh app instance, and unset it after the test.

### Live starts but the Chat immediately stops it

The owning Chat selection must pass through `AppState::live_voice_stops_for_chat_selection`. The helper preserves Live when the destination Chat ID equals the active Live Chat ID and stops it for every different destination.

### A durable command does not run while Live is active

The host executor must stop a competing Live call before executing the command. The one exemption is the exact command ID owned by the active Live delegation. Inspect delegation ownership rather than adding an ID-prefix heuristic.

### Casual speech appears in durable history

That violates the feature boundary. Only `live_delegation_created` should create `SessionCommandPayload::Run`. Phase, levels, and transcript events must remain in `LiveVoiceState` and must not be folded into the Chat document.

## Deliberate limitations

- Local device only.
- OMP only.
- One Live call per device.
- One unresolved delegation per call.
- OMP owns audio devices; Comet has no PCM transport or audio dependency.
- No public OpenAI Realtime API fallback.
- No automatic local dictation fallback.
- Realtime casual conversation is intentionally not a durable Chat transcript.

These constraints are architectural boundaries, not incomplete placeholders.

## Related records

- Historical architecture and decisions: [`plans/2026-08-31-omp-live-voice-design.md`](plans/2026-08-31-omp-live-voice-design.md).
- Voice-first new-Chat implementation plan: [`superpowers/plans/2026-09-01-new-chat-live-first.md`](superpowers/plans/2026-09-01-new-chat-live-first.md).
- Canonical OpenSpec capability: [`../openspec/specs/omp-live-voice/spec.md`](../openspec/specs/omp-live-voice/spec.md).
- Accepted and archived change: [`../openspec/changes/archive/2026-09-01-add-omp-live-voice/`](../openspec/changes/archive/2026-09-01-add-omp-live-voice/).

Evidence status: Confirmed unless noted.
