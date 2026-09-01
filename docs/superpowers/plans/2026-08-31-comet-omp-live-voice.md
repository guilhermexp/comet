# Comet OMP Live Voice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a local macOS Live Voice surface for OMP Chats while preserving Comet's durable command queue, normal OmpHarness run path, Run Journal, Chat Transcript, and sync boundaries.

**Architecture:** Comet launches one ephemeral OMP child as the voice frontend. When that child emits a delegation, `SessionsEngine` queues an ordinary durable Run command and the existing OmpHarness launches the backend child. Existing `AgentEvent` folding persists the result. A transient observer converts bounded backend commentary/final text into `live_append_context` controls for the voice child. No audio enters Comet.

**Tech Stack:** Rust, Tokio, serde, existing OMP JSONL RPC, existing `SessionsEngine`, Loro-backed command ledger, gpui, macOS app packaging.

**Spec:** `docs/plans/2026-08-31-omp-live-voice-design.md`

## Global Constraints

- Invoke the repository's `implement`, `test-driven-development`, and `ponytail` skills before editing production code.
- Create and strictly validate the OpenSpec change before production code.
- Preserve dependency direction: `proto → doc → sync → harness → engine → rpc → ui`.
- Voice is available only for a locally hosted, non-archived OMP Chat with no active backend run.
- At most one Live call exists per device and one backend delegation exists per Live call.
- Voice delegations use `SessionCommandPayload::Run`; no direct call may bypass the durable command ledger.
- The active runtime stores its exact owned command ID. Do not add a durable command-origin field or infer ownership from an ID prefix.
- The normal OmpHarness backend path remains the sole `AgentEvent` normalizer/folder.
- Casual captions, levels, phases, Live errors, and audio are never persisted or relayed.
- RPC methods are device-local and must not be added to `forwardable()`.
- No audio crate, public Realtime API, retry layer, or speculative TCC fallback.
- Use the signed Finder-launched `.app` for the final microphone/TCC smoke.

---

## File Structure

- Create `openspec/changes/add-omp-live-voice/`: proposal, design, tasks, and capability delta.
- Create `crates/proto/src/live_voice.rs`; modify `crates/proto/src/lib.rs`: shared serialized state types.
- Modify `crates/harness/src/lib.rs`: optional Live frontend contract with unsupported defaults.
- Modify `crates/harness/src/omp/{mod.rs,process.rs,protocol.rs}`: capability discovery, command encoding, transient event stream.
- Modify `crates/harness/tests/omp_rpc.rs` and `crates/harness/tests/fixtures/fake-omp-rpc.sh`: executable contract fixture.
- Create `crates/engine/src/live_voice.rs`; modify `crates/engine/src/{lib.rs,sessions.rs,doc_host.rs,rpc.rs}`: local lifecycle, durable delegation, preemption, and RPC.
- Modify `crates/rpc/src/lib.rs`: local method names.
- Create `crates/ui/src/live_voice.rs`; modify `crates/ui/src/{lib.rs,state.rs,composer.rs,shell.rs,sound.rs,icons.rs}`: control, strip, lifecycle actions, and sound suppression.
- Create `crates/ui/assets/icons/microphone.svg`.
- Modify `dist/macos/Info.plist`: microphone purpose string.

### Task 1: Specify the capability in OpenSpec

**Files:**
- Create: `openspec/changes/add-omp-live-voice/proposal.md`
- Create: `openspec/changes/add-omp-live-voice/design.md`
- Create: `openspec/changes/add-omp-live-voice/tasks.md`
- Create: `openspec/changes/add-omp-live-voice/specs/omp-live-voice/spec.md`

- [ ] **Step 1: Write the proposal and design**

`proposal.md` must state:

- OMP already owns a private subscription-backed Live media implementation;
- Comet lacks a desktop control because OMP RPC does not expose it;
- scope is local OMP Chats on macOS;
- delegated work remains durable through the existing command queue;
- out of scope: other harnesses, remote voice, audio relay/storage, public Realtime API, and Comet-owned capture.

`design.md` must reference `docs/plans/2026-08-31-omp-live-voice-design.md` and record the two-process decision: ephemeral Live frontend plus ordinary durable OmpHarness backend run.

- [ ] **Step 2: Write the normative capability delta**

In `specs/omp-live-voice/spec.md`, define these requirements with one `#### Scenario:` each and a `Test:` line matching repository OpenSpec rules:

```md
## ADDED Requirements

### Requirement: Local OMP Live availability
The system SHALL offer Live Voice only when the selected Chat is hosted on the current device, uses OMP, is not archived, has no active run, no other Live call exists, and the installed OMP advertises `liveVoice`.

#### Scenario: Unsupported OMP is rejected
- **WHEN** capability probing returns no `liveVoice`
- **THEN** start SHALL fail with an actionable OMP update reason and SHALL NOT mutate the Chat
- **Test:** engine integration

### Requirement: Media remains inside OMP
The system SHALL transport only control, phase, level, transcript, delegation, and terminal frames between Comet and the Live child.

#### Scenario: Live conversation remains transient
- **WHEN** user and assistant exchange realtime speech without a delegation
- **THEN** no Chat message, command, CRDT field, DeviceRoom frame, upload, or log SHALL contain audio or casual transcript content
- **Test:** harness integration + engine integration

### Requirement: Delegations use the durable run path
The system SHALL convert one Live delegation into one idempotent `SessionCommandPayload::Run` and SHALL execute it through the existing host executor and `SessionsEngine` pipeline.

#### Scenario: Delegated coding work is persisted once
- **WHEN** the Live child emits one delegation
- **THEN** exactly one user entry and the normal backend transcript SHALL be durable, while spoken paraphrases SHALL remain transient
- **Test:** engine integration

### Requirement: Competing commands stop Live
The system SHALL stop an active Live frontend before executing any durable command other than the exact command owned by its active delegation.

#### Scenario: Text command arrives while Live is active
- **WHEN** the host executor receives a different command ID
- **THEN** Live SHALL release microphone/playback before that command executes
- **Test:** engine integration

### Requirement: Device-local lifecycle
The system SHALL keep Live operations local to the host engine and SHALL release the Live child on End, Escape, Chat switch, surface close, engine shutdown, transport failure, or app quit.

#### Scenario: Repeated stop
- **WHEN** stop is called after Live has already ended
- **THEN** it SHALL succeed without spawning work or emitting duplicate terminal state
- **Test:** engine unit

### Requirement: macOS microphone declaration
The packaged macOS application SHALL declare a microphone purpose string and SHALL be smoke-tested from a signed Finder-launched app.

#### Scenario: Packaged permission grant
- **WHEN** a user starts Live from the signed app for the first time
- **THEN** macOS SHALL present an attributable microphone permission flow and successful grant SHALL allow OMP capture
- **Test:** manual packaged smoke
```

- [ ] **Step 3: Write implementation tasks with stable IDs**

`tasks.md` must mirror Tasks 2–9 below as C1–C8 and include exact verification commands.

- [ ] **Step 4: Strictly validate OpenSpec**

```bash
openspec validate add-omp-live-voice --strict --no-interactive
```

Expected: exits 0.

- [ ] **Step 5: Commit the specification**

```bash
git add openspec/changes/add-omp-live-voice
git commit -m "spec: add OMP live voice"
```

### Task 2: Add shared Live types and the optional harness contract

**Files:**
- Create: `crates/proto/src/live_voice.rs`
- Modify: `crates/proto/src/lib.rs`
- Modify: `crates/harness/src/lib.rs`

**Interfaces:**
- Produces: serialized UI/engine state in `zeron-proto`; transient harness stream/control types in `zeron-harness`.
- Keeps vendor frames out of `proto`.

- [ ] **Step 1: Write failing proto serialization tests**

In `crates/proto/src/live_voice.rs`, start with tests for the intended JSON shape:

```rust
#[test]
fn live_voice_state_serializes_camel_case() {
    let state = LiveVoiceState {
        chat_id: Some("chat-1".into()),
        phase: LiveVoicePhase::Working,
        muted: false,
        input_level: 0.25,
        output_level: 0.5,
        transcript: Some(LiveVoiceTranscript {
            role: LiveVoiceRole::User,
            turn: 2,
            text: "Inspect auth".into(),
            final_text: true,
        }),
        error: None,
    };
    let value = serde_json::to_value(state).unwrap();
    assert_eq!(value["chatId"], "chat-1");
    assert_eq!(value["phase"], "working");
    assert_eq!(value["transcript"]["finalText"], true);
}
```

Also round-trip every `LiveVoiceUnavailableReason` variant.

- [ ] **Step 2: Run the proto test and confirm RED**

```bash
cargo test -p zeron-proto live_voice
```

Expected: FAIL because the module/types are missing.

- [ ] **Step 3: Implement the shared state types**

Define:

```rust
pub enum LiveVoicePhase { Idle, Connecting, Listening, Speaking, Working, Muted, Stopping, Error }
pub enum LiveVoiceRole { User, Assistant }
pub enum LiveVoiceUnavailableReason { RemoteChat, NonOmp, Archived, ActiveRun, UnsupportedOmp, AnotherLiveCall }
pub struct LiveVoiceTranscript { pub role: LiveVoiceRole, pub turn: u64, pub text: String, pub final_text: bool }
pub struct LiveVoiceState {
    pub chat_id: Option<String>,
    pub phase: LiveVoicePhase,
    pub muted: bool,
    pub input_level: f32,
    pub output_level: f32,
    pub transcript: Option<LiveVoiceTranscript>,
    pub error: Option<String>,
}
pub struct LiveVoiceAvailability { pub available: bool, pub reason: Option<LiveVoiceUnavailableReason> }
```

Use camelCase structs and lower-camel enum values. Implement `Default` for idle state only.

- [ ] **Step 4: Write failing harness-default tests**

Add a tiny harness in `crates/harness/src/lib.rs` tests and assert:

- `probe_live_voice` returns `false`;
- `start_live_voice` returns `HarnessError::Unsupported`;
- normal `run` behavior is unaffected.

- [ ] **Step 5: Add transient harness types and defaults**

Add `HarnessError::Unsupported(String)` and:

```rust
pub struct LiveVoiceRequest { pub cwd: String }
pub enum LiveVoiceContextKind { Progress, Final }
pub enum LiveVoiceControl {
    SetMuted(bool),
    AppendContext { delegation_id: String, kind: LiveVoiceContextKind, text: String },
    Stop,
}
pub enum LiveVoiceEvent {
    Phase(LiveVoicePhase),
    Levels { input: f32, output: f32 },
    Transcript(LiveVoiceTranscript),
    Delegation { delegation_id: String, request: String },
    Ended { error: Option<String> },
}
pub struct LiveVoiceHandle {
    pub events: BoxStream<'static, Result<LiveVoiceEvent, HarnessError>>,
    pub controls: mpsc::Sender<LiveVoiceControl>,
}
```

Extend `Harness` with default methods:

```rust
async fn probe_live_voice(&self, _cwd: &Path) -> Result<bool, HarnessError> { Ok(false) }
async fn start_live_voice(&self, _request: LiveVoiceRequest) -> Result<LiveVoiceHandle, HarnessError> {
    Err(HarnessError::Unsupported(format!("{} does not support Live Voice", self.display_name())))
}
```

- [ ] **Step 6: Run and commit**

```bash
cargo test -p zeron-proto live_voice
cargo test -p zeron-harness live_voice_defaults
git add crates/proto/src/live_voice.rs crates/proto/src/lib.rs crates/harness/src/lib.rs
git commit -m "feat(voice): define shared live contracts"
```

### Task 3: Parse OMP Live capability and frames

**Files:**
- Modify: `crates/harness/src/omp/process.rs`
- Modify: `crates/harness/src/omp/protocol.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh`

**Interfaces:**
- Produces: `OmpCapabilities.live_voice`, typed transient events, and exact command encoders.

- [ ] **Step 1: Make the fixture advertise and exercise Live**

Change its ready frame to:

```sh
emit '{"type":"ready","protocolVersion":1,"supportedProtocolVersions":[1,2],"capabilities":{"liveVoice":1}}'
```

Add fixture branches:

- `live_start`: require `"delegationMode":"host"`, respond success, emit connecting/listening/caption/delegation frames;
- `live_set_muted`: echo success;
- `live_append_context`: require matching `delegationId`; emit listening after `kind: final`; echo success;
- `live_stop`: respond success and emit `live_ended`.

Use the existing depth-aware `field`/`has` helpers; do not add grep/sed JSON parsing.

- [ ] **Step 2: Write failing protocol tests**

In `omp_rpc.rs`, add tests that:

- a ready frame with `liveVoice: 1` produces `capabilities().live_voice == true`;
- absent capabilities produce `false`;
- malformed levels, transcript roles, empty delegation IDs, or unknown phases return protocol errors;
- unknown additive event types return `Ok(None)`;
- `live_append_context` serialization contains no unrelated fields.

- [ ] **Step 3: Run focused tests and confirm RED**

```bash
cargo test -p zeron-harness omp_live_protocol
```

Expected: FAIL because capability retention and Live parsing do not exist.

- [ ] **Step 4: Retain ready capabilities in `OmpProcess`**

Replace the ready channel's bare `bool` with:

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct OmpCapabilities {
    pub chunked_frames: bool,
    pub live_voice: bool,
}
```

Store capabilities on `OmpProcess`, copy them in `Clone`, negotiate v2 when `chunked_frames`, and expose `pub fn capabilities(&self) -> OmpCapabilities`.

The parser must treat only numeric `1` as support. Missing, null, boolean, or other versions mean unsupported rather than fatal.

- [ ] **Step 5: Add pure Live event and command conversion**

In `protocol.rs`, add:

```rust
pub fn parse_live_event(frame: &Value) -> Result<Option<LiveVoiceEvent>, HarnessError>
pub fn live_start_command() -> Value
pub fn live_mute_command(muted: bool) -> Value
pub fn live_context_command(delegation_id: &str, kind: LiveVoiceContextKind, text: &str) -> Result<Value, HarnessError>
pub fn live_stop_command() -> Value
```

Validate:

- levels are finite and clamp to `0.0..=1.0`;
- transcript/delegation text is non-empty and bounded by the existing inbound frame ceiling;
- IDs are non-empty and at most 256 bytes;
- context text is non-empty before sending;
- `live_ended.error` is sanitized before it reaches user-visible state.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p zeron-harness omp_live_protocol
git add crates/harness/src/omp/process.rs crates/harness/src/omp/protocol.rs crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh
git commit -m "feat(omp): parse live voice RPC frames"
```

### Task 4: Implement the OMP Live frontend handle

**Files:**
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh`

**Interfaces:**
- Implements: `Harness::probe_live_voice` and `Harness::start_live_voice` for `OmpHarness`.
- Preserves: normal `OmpHarness::run` setup and normalization unchanged.

- [ ] **Step 1: Write failing executable contract tests**

Add tests using the fake executable:

1. probe starts an ephemeral child, returns true, and shuts it down;
2. Live starts with `--no-session` and sends no `switch_session`, `set_model`, `set_thinking_level`, `set_host_tools`, or `prompt`;
3. phase/levels/transcript/delegation arrive in order;
4. progress/final controls encode matching IDs;
5. two serial delegations reuse the same Live child;
6. stop emits one terminal event and reaps the child;
7. unexpected child exit returns a bounded protocol error.

Use `FAKE_OMP_PID_FILE` to assert process reuse/reaping.

- [ ] **Step 2: Run tests and confirm RED**

```bash
cargo test -p zeron-harness omp_live_frontend
```

Expected: FAIL because OmpHarness uses default unsupported methods.

- [ ] **Step 3: Implement capability probe**

`probe_live_voice` launches `OmpProcess` with `ephemeral: true`, reads `process.capabilities().live_voice`, and always calls `shutdown()` before returning. Preserve the capability result if shutdown itself succeeds; propagate launch/handshake failure.

- [ ] **Step 4: Implement the Live stream/control task**

`start_live_voice` must:

1. validate/convert cwd;
2. launch an ephemeral process;
3. reject and shut down if `live_voice` is false;
4. take the event receiver;
5. send `live_start` with host delegation mode;
6. return a bounded `mpsc` control sender and a stream driven by one task.

The task `tokio::select!`s between OMP frames and controls. It routes controls through `OmpProcess::request`, parses only Live events, ignores unrelated additive frames, and shuts down on terminal event/error/Stop. A successful Stop yields exactly one `LiveVoiceEvent::Ended { error: None }` even if OMP also queued its terminal frame.

Use a bounded control channel (capacity 16). Levels are transient: if the UI is slow, coalesce/drop stale level updates rather than growing memory; never drop phase, transcript, delegation, or terminal events.

- [ ] **Step 5: Verify normal OMP regression behavior**

```bash
cargo test -p zeron-harness omp_live_frontend
cargo test -p zeron-harness --test omp_rpc full_rpc_run_normalizes_events_and_resumes_session
```

Expected: PASS; the existing normal run still resumes/configures/prompts exactly once.

- [ ] **Step 6: Commit**

```bash
git add crates/harness/src/omp/mod.rs crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh
git commit -m "feat(omp): add live voice frontend handle"
```

### Task 5: Add engine-owned local Live lifecycle

**Files:**
- Create: `crates/engine/src/live_voice.rs`
- Modify: `crates/engine/src/lib.rs`
- Modify: `crates/engine/src/sessions.rs`

**Interfaces:**
- Produces: probe/start/mute/stop/watch operations and one-device runtime ownership.
- Does not yet queue delegations; Task 6 adds that path.

- [ ] **Step 1: Write state-machine unit tests**

In `live_voice.rs`, test a pure `LiveVoiceCoordinator` with fake controls:

- idle → connecting → listening;
- levels/captions update watch state without durable hooks;
- mute preserves the prior active phase and sends one exact control;
- second start is rejected;
- stop transitions through stopping to idle;
- repeated stop is successful;
- terminal error emits error once and clears ownership.

- [ ] **Step 2: Run and confirm RED**

```bash
cargo test -p zeron-engine live_voice_state
```

Expected: FAIL because the module is absent.

- [ ] **Step 3: Implement coordinator state and bounded text policy**

`LiveVoiceCoordinator` owns:

```rust
watch::Sender<LiveVoiceState>
Mutex<Option<ActiveLiveVoice>>
```

`ActiveLiveVoice` contains call ID, chat ID, control sender, optional active delegation/owned command ID, and task cancellation/join handle.

Store only the latest transcript and levels. Cap user-visible transcript/context text at the existing RPC frame limit or a smaller explicit constant (64 KiB); do not retain transcript history.

- [ ] **Step 4: Add `SessionsEngine` lifecycle methods**

Expose:

```rust
pub async fn probe_live_voice(&self, chat_id: &str) -> Result<LiveVoiceAvailability, EngineError>;
pub async fn start_live_voice(&self, chat_id: &str) -> Result<(), EngineError>;
pub async fn set_live_voice_muted(&self, muted: bool) -> Result<(), EngineError>;
pub async fn stop_live_voice(&self) -> Result<(), EngineError>;
pub fn watch_live_voice(&self) -> watch::Receiver<LiveVoiceState>;
```

Preconditions read the authoritative workspace Chat row and enforce local device, OMP, non-archived, no active run, no other Live call, then use registry OMP capability probing. Start builds only `LiveVoiceRequest { cwd }`.

- [ ] **Step 5: Add engine tests with a fake Live-capable harness**

Register a fake OMP harness and assert every unavailable reason plus successful start/mute/stop. Assert Chat docs and Run Journal remain byte/sequence unchanged after phase/levels/transcript-only events.

- [ ] **Step 6: Run and commit**

```bash
cargo test -p zeron-engine live_voice_state
cargo test -p zeron-engine live_voice_preconditions
git add crates/engine/src/live_voice.rs crates/engine/src/lib.rs crates/engine/src/sessions.rs
git commit -m "feat(engine): own local live voice lifecycle"
```

### Task 6: Route delegations through the durable command ledger

**Files:**
- Modify: `crates/engine/src/live_voice.rs`
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/engine/src/doc_host.rs`
- Test: `crates/engine/src/live_voice.rs`
- Test: `crates/engine/tests/e2e.rs`

**Interfaces:**
- Consumes: `LiveVoiceEvent::Delegation` and the existing `SessionsEngine` broadcast hub.
- Produces: one exact `SessionCommandPayload::Run`, progress/final controls, and command preemption.

- [ ] **Step 1: Test pure backend speech accumulation**

Create `BackendSpeechAccumulator` and tests:

- `TextDelta` appends visible assistant text;
- `AssistantMessageCompleted` returns one trimmed progress value and clears the current segment;
- reasoning, tools, usage, questions, and subagent events do not become spoken text;
- `Done.result` wins as final text;
- otherwise the last non-empty assistant segment is final;
- errored Done uses its actual error text;
- completed Done with no text returns the factual sentence `The coding run completed without a final text response.`.

- [ ] **Step 2: Write failing end-to-end delegation tests**

Using a fake Live-capable OMP harness plus the existing instant backend pattern, assert:

1. Live delegation subscribes to the chat hub before queueing;
2. command ID and message ID are stable for the call/delegation pair;
3. exactly one pending Run command appears despite duplicate delegation delivery;
4. host execution creates exactly one user entry and one normal assistant entry;
5. progress/final controls carry the original delegation ID;
6. no spoken context is appended to the Chat;
7. a second delegation while one is active ends Live with a protocol error rather than creating concurrent work.

- [ ] **Step 3: Run and confirm RED**

```bash
cargo test -p zeron-engine live_voice_delegation
```

Expected: FAIL because delegation events are not queued.

- [ ] **Step 4: Derive the ordinary backend request from the Chat**

Add a pure helper using authoritative `Chat` fields:

```rust
RunRequest {
    prompt: request,
    harness: Some(HarnessId::Omp),
    model: chat.config.as_ref().and_then(|c| c.model.clone()),
    reasoning: chat.config.as_ref().and_then(|c| c.reasoning),
    model_options: chat.config.as_ref().map(|c| c.model_options.clone()).unwrap_or_default(),
    cwd: chat
        .source_context
        .as_ref()
        .map(|context| context.cwd.clone())
        .or_else(|| chat.cwd.clone())
        .ok_or_else(|| EngineError::Other("Live Voice requires a Chat checkout cwd".into()))?,
    sandbox: chat.config.as_ref().map(|c| c.sandbox).unwrap_or(SandboxLevel::WorkspaceWrite),
    auto_approve: false,
    enable_workers_mcp: true,
    workers_parent_chat_id: None,
    resume: None,
    attachments: Vec::new(),
    worktree: None,
}
```

The host executor remains responsible for resume injection and checkout/worktree policy.

- [ ] **Step 5: Queue idempotently and observe the existing run**

On delegation:

1. reject if active delegation exists;
2. subscribe with `sessions.subscribe(chat_id, u64::MAX)` before queueing;
3. create opaque stable IDs with `new_id()` once and store them in runtime;
4. call `DocHost::queue_command_with_id` with `SessionCommandPayload::Run`;
5. consume only future broadcast events until top-level `AgentEvent::Done`;
6. send progress/final through the Live control channel;
7. clear delegation ownership after final append succeeds.

Duplicate event delivery reuses the stored command ID and cannot create a second doc command.

- [ ] **Step 6: Stop Live before every non-owned durable command**

Add:

```rust
pub async fn prepare_for_command(&self, chat_id: &str, command_id: &str) -> Result<(), EngineError>
```

It returns immediately only when both chat ID and command ID match the active voice delegation. Otherwise, if any Live runtime exists, it awaits `stop_live_voice()`.

Call this in `DocHost` immediately before executing an accepted command and before `SessionsEngine::dispatch`/steer/interrupt/respond-input routing. Do not hold the doc handle or command ledger lock while awaiting stop.

- [ ] **Step 7: Test preemption and backend independence**

Assert:

- the owned voice command does not stop Live;
- a different command waits for Stop before dispatch;
- if the Live child dies after queueing, the backend run still completes durably;
- if the backend run errors, its normal durable error remains and a final failure context is attempted only while Live is connected.

- [ ] **Step 8: Run and commit**

```bash
cargo test -p zeron-engine live_voice_delegation
cargo test -p zeron-engine --test e2e deterministic_queue_command_id_is_returned_and_executes_once
git add crates/engine/src/live_voice.rs crates/engine/src/sessions.rs crates/engine/src/doc_host.rs crates/engine/tests/e2e.rs
git commit -m "feat(engine): queue live voice delegations durably"
```

### Task 7: Expose local-only Engine RPC methods

**Files:**
- Modify: `crates/rpc/src/lib.rs`
- Modify: `crates/engine/src/rpc.rs`

**Interfaces:**
- Produces: probe/start/mute/stop/watch API for gpui.
- Prohibits: relay forwarding.

- [ ] **Step 1: Write failing method-routing tests**

In `crates/engine/src/rpc.rs` tests, assert the new constants exist and all return `false` from `forwardable()`:

```rust
assert!(!forwardable(methods::PROBE_LIVE_VOICE));
assert!(!forwardable(methods::START_LIVE_VOICE));
assert!(!forwardable(methods::SET_LIVE_VOICE_MUTED));
assert!(!forwardable(methods::STOP_LIVE_VOICE));
assert!(!forwardable(methods::WATCH_LIVE_VOICE));
```

Add handler tests for local Chat success, remote Chat rejection, exact mute value, repeated stop, and a watch reset/update frame.

- [ ] **Step 2: Run and confirm RED**

```bash
cargo test -p zeron-engine live_voice_rpc
```

Expected: FAIL.

- [ ] **Step 3: Add method constants and typed parameters**

In `zeron-rpc` add:

```rust
pub const PROBE_LIVE_VOICE: &str = "ProbeLiveVoice";
pub const START_LIVE_VOICE: &str = "StartLiveVoice";
pub const SET_LIVE_VOICE_MUTED: &str = "SetLiveVoiceMuted";
pub const STOP_LIVE_VOICE: &str = "StopLiveVoice";
pub const WATCH_LIVE_VOICE: &str = "WatchLiveVoice";
```

Use `{ chatId }` for probe/start, `{ muted }` for mute, and empty params for stop/watch. Return `LiveVoiceAvailability`, `{ active: true }`, `{ muted }`, `{ active: false }`, and streamed `LiveVoiceState` respectively.

- [ ] **Step 4: Wire handlers and shutdown**

Route directly to `SessionsEngine`. Do not add methods to forwardable lists/deadlines. Ensure engine shutdown calls `stop_live_voice()` before clearing the sessions/doc-host back edge.

- [ ] **Step 5: Run and commit**

```bash
cargo test -p zeron-engine live_voice_rpc
cargo test -p zeron-rpc
git add crates/rpc/src/lib.rs crates/engine/src/rpc.rs
git commit -m "feat(rpc): expose local live voice controls"
```

### Task 8: Build the native gpui Live surface

**Files:**
- Create: `crates/ui/src/live_voice.rs`
- Create: `crates/ui/assets/icons/microphone.svg`
- Modify: `crates/ui/src/lib.rs`
- Modify: `crates/ui/src/icons.rs`
- Modify: `crates/ui/src/state.rs`
- Modify: `crates/ui/src/composer.rs`
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/sound.rs`

**Interfaces:**
- Consumes: local Live RPC/watch state.
- Produces: microphone button, stable active strip, accessibility labels, lifecycle controls.

- [ ] **Step 1: Test pure availability and view-model derivation**

In `live_voice.rs`, define a pure `LiveVoiceViewModel` and test:

- available local OMP Chat shows enabled microphone;
- each unavailable reason maps to one actionable string;
- active state for another Chat disables start;
- working/muted/speaking labels are stable;
- latest caption coalesces by role/turn;
- levels clamp before rendering;
- active state replaces composer input rather than overlaying it.

- [ ] **Step 2: Test global notification-sound suppression**

In `sound.rs`, add an `AtomicBool`-backed `set_live_voice_active(bool)` and test that `play`'s decision helper refuses both Done and Request sounds while active, then restores them after stop. Keep actual platform audio commands outside the unit test.

- [ ] **Step 3: Run and confirm RED**

```bash
cargo test -p zeron-ui live_voice
```

Expected: FAIL because UI module/state is absent.

- [ ] **Step 4: Register the microphone asset**

Create a single monochrome 24×24 SVG matching existing icon stroke/currentColor conventions. Add `MICROPHONE` to `icons.rs`. Mute state uses the same icon plus selected styling and `Mute`/`Unmute` text; do not add a second asset unless visual verification proves ambiguity.

- [ ] **Step 5: Add AppState lifecycle and watch ownership**

`AppState` owns:

```rust
pub live_voice: LiveVoiceState
pub live_voice_availability: Option<LiveVoiceAvailability>
live_voice_watch_task: Option<Task<()>>
live_voice_probe_task: Option<Task<()>>
```

Attach `WATCH_LIVE_VOICE` once after engine connection. Probe when selected Chat identity/host/harness changes, not on render. Add methods for start, exact mute, stop, and Escape. Selection change and app/window close send Stop when active. Every state update calls `sound::set_live_voice_active(state.phase != Idle)`.

- [ ] **Step 6: Render the inactive microphone control**

Add the button to the existing composer action cluster. Requirements:

- 32 px target matching adjacent actions;
- tooltip/accessible label `Start Live Voice`;
- disabled tooltip is the exact derived reason;
- no control on non-OMP or remote Chat if the current composer convention hides unsupported actions; otherwise disabled consistently with adjacent controls.

Click calls `StartLiveVoice` only; UI does not optimistically claim microphone ownership before the engine watch update.

- [ ] **Step 7: Render one stable active strip**

When `live_voice.chat_id == selected_chat` and phase is active, replace the editor body with:

- phase/status text;
- two lightweight level bars or one combined visualization driven by `input_level`/`output_level`;
- latest caption truncated visually, not in state;
- Mute/Unmute button with `aria-pressed` equivalent;
- End button;
- Escape handling that calls Stop.

Keep the normal Chat transcript visible. Do not store captions in editor buffers or message state.

- [ ] **Step 8: Run UI tests and build**

```bash
cargo test -p zeron-ui live_voice
cargo build -p zeron
```

Expected: PASS.

- [ ] **Step 9: Visually verify the real gpui surface**

Run:

```bash
scripts/dev-demo.sh
```

In a local OMP Chat verify inactive, connecting, listening, working, speaking, muted, long caption, narrow window, disabled older-OMP, and active-other-Chat states. Confirm keyboard focus, Escape, accessibility labels, and no layout shift in the action cluster. Save screenshots in the repository's existing output location, not source directories.

- [ ] **Step 10: Commit UI**

```bash
git add crates/ui/src/live_voice.rs crates/ui/assets/icons/microphone.svg crates/ui/src/lib.rs crates/ui/src/icons.rs crates/ui/src/state.rs crates/ui/src/composer.rs crates/ui/src/shell.rs crates/ui/src/sound.rs
git commit -m "feat(ui): add OMP live voice surface"
```

### Task 9: Package, smoke, review, and close the change

**Files:**
- Modify: `dist/macos/Info.plist`
- Modify: `openspec/changes/add-omp-live-voice/tasks.md`

- [ ] **Step 1: Add the macOS microphone purpose string**

Add inside the root `<dict>`:

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Comet uses the microphone for realtime voice conversations with your coding agent.</string>
```

Do not add audio background modes or entitlements.

- [ ] **Step 2: Run the complete targeted verification gate**

```bash
cargo fmt --all
cargo test -p zeron-proto live_voice
cargo test -p zeron-harness omp_live
cargo test -p zeron-engine live_voice
cargo test -p zeron-ui live_voice
cargo test -p zeron-rpc
cargo build -p zeron
cargo clippy -p zeron-harness -p zeron-engine -p zeron-ui --all-targets -- -D warnings
openspec validate add-omp-live-voice --strict --no-interactive
```

Expected: all exit 0.

- [ ] **Step 3: Build and launch the packaged app**

```bash
scripts/package-macos.sh
```

Sign using the repository's existing packaging identity flow. Launch the resulting `.app` through Finder, not `cargo run` or Terminal.

- [ ] **Step 4: Perform the real TCC/media smoke**

From the signed Finder-launched app:

1. select a local OMP Chat;
2. start Live and accept the microphone prompt;
3. speak casually and confirm the Chat/Run Journal do not change;
4. request a real repository operation;
5. observe exactly one durable user request plus normal tools/final response;
6. hear backend progress/final through OMP Live;
7. interrupt Live speech, mute, and unmute;
8. queue a normal text command from another viewport and verify Live ends before execution;
9. switch Chat and verify the Live strip/microphone indicator clear;
10. quit Comet and verify no OMP child retains microphone ownership.

If TCC does not authorize the child topology, stop release work and revise the approved design. Do not implement Comet-owned PCM capture in this change.

- [ ] **Step 5: Review for privacy and lifecycle regressions**

Inspect the diff for:

- logged transcripts/tokens/audio;
- forwarded Live methods;
- unbounded channels/buffers;
- duplicate command IDs/messages;
- locks held across process stop;
- OMP children surviving every terminal path;
- normal OmpHarness setup changes unrelated to Live.

Run the repository's required code-review skill and resolve findings before proceeding.

- [ ] **Step 6: Mark OpenSpec tasks complete and revalidate all changes**

Update `openspec/changes/add-omp-live-voice/tasks.md`, then run:

```bash
openspec validate --all --strict
```

Expected: exits 0.

- [ ] **Step 7: Commit packaging and closure**

```bash
git add dist/macos/Info.plist openspec/changes/add-omp-live-voice/tasks.md
git commit -m "chore(macos): enable OMP live microphone access"
```

## Comet Plan Completion Gate

- OMP capability is probed, not inferred from version text.
- Live is local-only, OMP-only, and one-per-device.
- Casual voice and audio never enter a durable or relay path.
- One delegation produces one exact durable Run command and existing backend folding.
- Voice progress/result context is transient and correlation-safe.
- A different durable command stops Live before execution.
- Normal OmpHarness runs still resume/configure OMP exactly as before.
- UI owns pixels only; engine owns lifecycle and preconditions.
- Signed Finder-launched app passes microphone/TCC, delegation, preemption, and cleanup smoke.
- OpenSpec strict validation, targeted tests, build, clippy, visual check, and review all pass.
