# Live Voice During an Active Run Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow Live Voice to start while an OMP Chat Session is working, silently receive bounded operational context, answer status questions without mutating the Chat, and steer the active run only after explicit voice confirmation.

**Architecture:** OMP exposes its existing `session.context.append` primitive through an additive RPC capability and command. Comet permits Live start during active Sessions only when that capability is advertised, observes the originating Chat's visible event stream into a latest-value projection, and routes confirmed Live delegations through `SessionsEngine::steer` with the existing durable fallback.

**Tech Stack:** Rust 2024, Tokio `watch`/`broadcast`/`mpsc`, serde JSON, TypeScript, Bun/Vitest, OMP RPC UI protocol, gpui.

**Spec:** `docs/plans/2026-09-01-live-voice-during-active-run-design.md`

## Global Constraints

- `Chat` is the durable conversation; `Session` is its device-local execution state.
- The active call remains device-local and tied to its originating Chat.
- Activating Live or receiving context MUST NOT initiate speech, a delegation, or a second run.
- Casual voice transcripts and answers MUST NOT enter the Chat Transcript.
- Operational context MUST exclude audio, reasoning deltas, raw Run Journal data, and protected tool payloads/results.
- A possible instruction MUST be confirmed in Live before OMP emits a host delegation.
- A confirmed instruction MUST produce at most one durable user entry and one execution path.
- Operational context MUST never backpressure the coding run; stale pending snapshots are replaceable.
- Old OMP versions retain idle Live Voice but MUST NOT be offered Live during `Working` or `AwaitingInput`.
- Complete and validate an OpenSpec change before editing capability behavior; archive it only after the real-app smoke passes.
- Implement the currently uncommitted `fix-live-voice-delegation-continuity` baseline first or preserve its navigation and parked-run semantics throughout this work.

---

### Task 1: Define the OpenSpec capability delta

**Files:**
- Create: `openspec/changes/allow-live-voice-during-active-run/proposal.md`
- Create: `openspec/changes/allow-live-voice-during-active-run/design.md`
- Create: `openspec/changes/allow-live-voice-during-active-run/specs/omp-live-voice/spec.md`
- Create: `openspec/changes/allow-live-voice-during-active-run/tasks.md`

**Interfaces:**
- Consumes: approved behavior in `docs/plans/2026-09-01-live-voice-during-active-run-design.md`.
- Produces: requirements for active-Session availability, silent operational context, confirmed steering, and settlement fallback.

- [ ] **Step 1: Create the change artifacts**

Write the requirements with these normative scenarios:

```markdown
### Requirement: Live observes an active Session
The system SHALL allow Live Voice to start for an otherwise eligible local OMP Chat while its Session is `Working` or `AwaitingInput` when OMP advertises operational-context support. Context updates SHALL remain silent and transient.

#### Scenario: Working Session accepts Live
- **Test:** engine e2e + manual packaged smoke
- **WHEN** an OMP Session is Working and operational-context support is advertised
- **THEN** Live Voice SHALL start without interrupting or replacing the run
- **AND** silence SHALL produce no speech, delegation, or durable Chat entry

### Requirement: Active-run questions are read-only
The system SHALL provide Live with bounded visible operational context for its originating Chat without reading the raw Run Journal or mutating the Chat.

#### Scenario: Status question does not steer
- **Test:** OMP Live integration + engine e2e
- **WHEN** the user asks about current progress
- **THEN** Live SHALL answer from current operational context
- **AND** the active run and Chat Transcript SHALL remain unchanged

### Requirement: Active-run instructions require confirmation
The system SHALL require explicit Live confirmation before emitting a delegation while the observed Session is active.

#### Scenario: Confirmed instruction steers once
- **Test:** OMP Live integration + engine e2e
- **WHEN** the user confirms an instruction while the Session is Working
- **THEN** exactly one durable user entry SHALL be written
- **AND** exactly one steer SHALL enter the existing run

#### Scenario: Run settles during confirmation
- **Test:** engine e2e
- **WHEN** the observed run settles before a confirmed instruction can be steered
- **THEN** the instruction SHALL execute as exactly one normal durable turn
- **AND** it SHALL NOT also be steered
```

- [ ] **Step 2: Validate the proposed change**

Run:

```bash
openspec validate allow-live-voice-during-active-run --strict
```

Expected: the change is valid with no missing scenario metadata.

- [ ] **Step 3: Commit the OpenSpec artifacts**

```bash
git add openspec/changes/allow-live-voice-during-active-run
git commit -m "docs(live): specify voice during active runs"
```

---

### Task 2: Expose silent session context in OMP Live RPC

**Files:**
- Modify: `../oh-my-pi/packages/coding-agent/src/modes/rpc/rpc-types.ts`
- Modify: `../oh-my-pi/packages/coding-agent/src/modes/rpc/rpc-live.ts`
- Modify: `../oh-my-pi/packages/coding-agent/src/modes/rpc/rpc-mode.ts`
- Modify: `../oh-my-pi/packages/coding-agent/src/live/controller.ts`
- Modify: `../oh-my-pi/packages/coding-agent/src/live/prompts/live-instructions.md`
- Modify: `../oh-my-pi/packages/coding-agent/test/rpc-live.test.ts`
- Modify: `../oh-my-pi/packages/coding-agent/test/live/controller.test.ts`

**Interfaces:**
- Consumes: existing `buildSessionContextAppend(text, "commentary")` from `src/live/protocol.ts`.
- Produces: capability `liveVoiceSessionContext: 1` and RPC command `{ type: "live_append_session_context", text: string }`.

- [ ] **Step 1: Write failing RPC lifecycle tests**

Extend the existing `FakeLiveController` with:

```ts
sessionContexts: string[] = [];

async appendSessionContext(text: string): Promise<void> {
  this.sessionContexts.push(text);
}
```

Then add:

```ts
it("forwards silent session context to the active Live controller", async () => {
  const controller = new FakeLiveController();
  const lifecycle = createRpcLiveLifecycle(() => controller);
  await lifecycle.start();

  const result = await handleRpcLiveCommand(lifecycle, {
    type: "live_append_session_context",
    text: "Session status: Working\nCurrent action: Running tests",
  });

  expect(controller.sessionContexts).toEqual([
    "Session status: Working\nCurrent action: Running tests",
  ]);
  expect(result).toEqual({
    command: "live_append_session_context",
    data: { accepted: true },
  });
});
```

Also assert blank text is rejected and the command is rejected when Live is inactive.

- [ ] **Step 2: Run the focused OMP test and confirm RED**

Run from `../oh-my-pi`:

```bash
bun test packages/coding-agent/test/rpc-live.test.ts
```

Expected: FAIL because the command and controller method do not exist.

- [ ] **Step 3: Add the typed RPC command and capability**

Extend `RpcCommand`, `RpcResponse`, `RpcLiveController`, `RpcLiveLifecycle`, and `RpcLiveCommandResult` with:

```ts
| { id?: string; type: "live_append_session_context"; text: string }

appendSessionContext(text: string): Promise<void>;

| {
    command: "live_append_session_context";
    data: { accepted: true };
  }
```

Advertise the additive ready capability without changing `liveVoice: 1`:

```ts
export interface RpcCapabilities {
  liveVoice?: 1;
  liveVoiceSessionContext?: 1;
}
```

Set `liveVoiceSessionContext: 1` beside `liveVoice: 1` in the ready frame emitted by `src/modes/rpc/rpc-mode.ts`.

- [ ] **Step 4: Implement silent session-context append**

In `LiveSessionController`, add:

```ts
async appendSessionContext(text: string): Promise<void> {
  const normalized = text.trim();
  if (!normalized) throw new Error("Live session context must not be empty");
  let pending: Promise<void> = Promise.resolve();
  for (const chunk of chunkLiveContext(normalized)) {
    pending = this.#queueSend(buildSessionContextAppend(chunk, "commentary"));
  }
  await pending;
}
```

Import `buildSessionContextAppend`, expose the method through `createRpcLiveLifecycle`, and route `live_append_session_context` in `handleRpcLiveCommand`. Do not call `response.create`, synthesize audio, or emit `onDelegation`.

- [ ] **Step 5: Encode the confirmation policy in Live instructions**

Add a concise active-run rule to `live-instructions.md`:

```markdown
Host may append silent operational context for an already-running coding turn. Use it only to answer the user's questions; NEVER announce it proactively. While that context says work is active, a request that would change or add work MUST be confirmed aloud before creating a client delegation. Informational questions and rejected proposals MUST NOT create a delegation. Structured input or authorization waits remain owned by the client UI.
```

- [ ] **Step 6: Prove context append is silent and delegation-free**

In `test/live/controller.test.ts`, fake the transport send path, call `appendSessionContext`, and assert the only emitted client message is:

```ts
{
  type: "session.context.append",
  channel: "commentary",
  content: [{ type: "input_text", text: "Session status: Working" }],
}
```

Assert `onTranscript`, `onDelegation`, and output-audio callbacks are not called.

- [ ] **Step 7: Run focused OMP Live tests**

```bash
bun test \
  packages/coding-agent/test/rpc-live.test.ts \
  packages/coding-agent/test/live/controller.test.ts \
  packages/coding-agent/test/live/protocol.test.ts
```

Expected: PASS.

- [ ] **Step 8: Commit the OMP protocol extension in the OMP repository**

```bash
git add \
  packages/coding-agent/src/modes/rpc/rpc-types.ts \
  packages/coding-agent/src/modes/rpc/rpc-live.ts \
  packages/coding-agent/src/modes/rpc/rpc-mode.ts \
  packages/coding-agent/src/live/controller.ts \
  packages/coding-agent/src/live/prompts/live-instructions.md \
  packages/coding-agent/test/rpc-live.test.ts \
  packages/coding-agent/test/live/controller.test.ts
git commit -m "feat(live): accept silent active-run context"
```

---

### Task 3: Add Comet harness capability and control frames

**Files:**
- Modify: `crates/harness/src/lib.rs`
- Modify: `crates/harness/src/omp/process.rs`
- Modify: `crates/harness/src/omp/protocol.rs`
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/tests/omp_rpc.rs`
- Modify: `crates/engine/src/live_voice.rs` test harness
- Modify: `crates/engine/tests/e2e.rs` Live fixture

**Interfaces:**
- Consumes: OMP capability `liveVoiceSessionContext: 1` and command `live_append_session_context`.
- Produces: `LiveVoiceSupport { available, session_context }` and `LiveVoiceControl::AppendSessionContext { text }`.

- [ ] **Step 1: Write failing capability and protocol tests**

Add parser cases proving independent compatibility:

```rust
assert_eq!(
    parse_capabilities(&json!({
        "capabilities": {
            "liveVoice": 1,
            "liveVoiceSessionContext": 1
        }
    })),
    OmpCapabilities {
        live_voice: true,
        live_voice_session_context: true,
        ..OmpCapabilities::default()
    }
);
```

Add a serializer assertion:

```rust
assert_eq!(
    live_session_context_command("Session status: Working").unwrap(),
    json!({
        "type": "live_append_session_context",
        "text": "Session status: Working"
    })
);
```

Assert blank context is rejected.

- [ ] **Step 2: Run the focused harness tests and confirm RED**

```bash
cargo test -p zeron-harness omp -- --nocapture
```

Expected: FAIL because the capability field, support type, and command do not exist.

- [ ] **Step 3: Replace the Boolean probe result with explicit support**

Add in `crates/harness/src/lib.rs`:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveVoiceSupport {
    pub available: bool,
    pub session_context: bool,
}
```

Change the default harness probe signature to:

```rust
async fn probe_live_voice(&self, _cwd: &Path) -> Result<LiveVoiceSupport, HarnessError> {
    Ok(LiveVoiceSupport::default())
}
```

Update `OmpHarness::probe_live_voice` to return both parsed capability bits from one temporary OMP process.

Migrate the two repository test overrides in `crates/engine/src/live_voice.rs` and `crates/engine/tests/e2e.rs` to return:

```rust
LiveVoiceSupport {
    available: true,
    session_context: true,
}
```

- [ ] **Step 4: Add the silent control variant and JSON serializer**

Extend `LiveVoiceControl`:

```rust
AppendSessionContext {
    text: String,
},
```

Add `live_session_context_command(text: &str) -> Result<Value, HarnessError>` with the same non-empty validation used by delegation context. Route the new control in `run_live_voice` without changing phase or emitting a `LiveVoiceEvent`.

- [ ] **Step 5: Prove the OMP process receives the new control**

Extend the fake RPC fixture test to send:

```rust
controls
    .send(LiveVoiceControl::AppendSessionContext {
        text: "Session status: Working".into(),
    })
    .await
    .unwrap();
```

Assert the child receives `live_append_session_context`, responds successfully, and the Live event stream emits no transcript, delegation, or terminal event because of that control.

- [ ] **Step 6: Run the focused harness suite**

```bash
cargo test -p zeron-harness
```

Expected: PASS.

- [ ] **Step 7: Commit the Comet harness boundary**

```bash
git add \
  crates/harness/src/lib.rs \
  crates/harness/src/omp/process.rs \
  crates/harness/src/omp/protocol.rs \
  crates/harness/src/omp/mod.rs \
  crates/harness/tests/omp_rpc.rs \
  crates/engine/src/live_voice.rs \
  crates/engine/tests/e2e.rs
git commit -m "feat(harness): stream active-run context to Live"
```

---

### Task 4: Build the privacy-safe operational projection

**Files:**
- Modify: `crates/engine/src/live_voice.rs`
- Test: `crates/engine/src/live_voice.rs` unit module

**Interfaces:**
- Consumes: `AgentEvent`, `SessionStatus`, `SessionMessageEntry`, `MessagePart`, and `zeron_proto::view::tool_presentation`.
- Produces: `LiveOperationalContext::observe(&AgentEvent)`, `LiveOperationalContext::render()`, and `latest_visible_assistant_text(&[SessionMessageEntry])`; returned text is bounded by `MAX_LIVE_TEXT_BYTES`.

- [ ] **Step 1: Write failing projection tests**

Cover this event sequence:

```rust
let mut context = LiveOperationalContext::new(SessionStatus::Working, "Already visible");
context.observe(&AgentEvent::ToolCall {
    id: "tool-1".into(),
    call: ToolCall::Exec { command: "secret command".into() },
});
context.observe(&AgentEvent::TextDelta { text: "Tests are running.".into() });

let rendered = context.render();
assert!(rendered.contains("Session status: Working"));
assert!(rendered.contains("Current action: Running command"));
assert!(rendered.contains("Visible assistant update: Tests are running."));
assert!(!rendered.contains("secret command"));
```

Also prove:

- `ReasoningDelta` makes no change;
- `ToolResult.output` and `.diff` never enter rendered context;
- `InputRequested` renders only `Session status: AwaitingInput`, not question payloads;
- `Done` renders `Idle` or `Errored` and the visible terminal error;
- repeated text truncates on a UTF-8 boundary at `MAX_LIVE_TEXT_BYTES`.

For the initial snapshot, prove `latest_visible_assistant_text` concatenates only `MessagePart::Text` values from the newest assistant entry and excludes reasoning and tool parts.

- [ ] **Step 2: Run the focused projection test and confirm RED**

```bash
cargo test -p zeron-engine live_operational_context -- --nocapture
```

Expected: FAIL because `LiveOperationalContext` does not exist.

- [ ] **Step 3: Implement the projection with existing presentation policy**

Use a small state object rather than retaining raw events:

```rust
pub(crate) struct LiveOperationalContext {
    status: SessionStatus,
    visible_text: String,
    active_tool: Option<(String, &'static str)>,
    visible_error: Option<String>,
}
```

On `ToolCall`, retain only the id and `tool_presentation(call, false, false).label`. On matching `ToolResult`, clear the active tool and retain only `is_error`; never copy `output` or `diff`. Ignore `ReasoningDelta`, `Usage`, raw workflow payloads, and nested `Subagent` events. Reuse the existing bounded-string helper instead of adding another truncation implementation.

Implement the initial transcript projection without serializing the whole transcript:

```rust
pub(crate) fn latest_visible_assistant_text(entries: &[SessionMessageEntry]) -> String {
    let Some(entry) = entries
        .iter()
        .rev()
        .find(|entry| entry.role == MessageRole::Assistant)
    else {
        return String::new();
    };
    let mut text = String::new();
    for part in &entry.parts {
        if let MessagePart::Text { text: value, .. } = part {
            push_bounded(&mut text, value);
        }
    }
    text
}
```

Render stable labels in this order:

```text
Session status: Working
Current action: Running command
Visible assistant update: Tests are running.
Visible error: <only when present>
```

- [ ] **Step 4: Run projection tests**

```bash
cargo test -p zeron-engine live_operational_context -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit the isolated projection**

```bash
git add crates/engine/src/live_voice.rs
git commit -m "feat(engine): derive silent Live run context"
```

---

### Task 5: Allow active-Session Live and forward latest context

**Files:**
- Modify: `crates/proto/src/live_voice.rs`
- Modify: `crates/engine/src/live_voice.rs`
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/ui/src/live_voice.rs`
- Modify: `crates/engine/tests/e2e.rs`
- Modify: `crates/ui/src/live_voice.rs` unit module

**Interfaces:**
- Consumes: `LiveVoiceSupport.session_context`, `LiveOperationalContext`, and `LiveVoiceControl::AppendSessionContext`.
- Produces: active-run availability, a per-call latest-value context channel, and one observer bound to the originating Chat.

- [ ] **Step 1: Write failing active-run availability tests**

In engine e2e coverage, start a stable OMP run that remains `Working`, probe Live, and assert:

```rust
assert_eq!(engine.probe_live_voice(chat_id).await.unwrap(), LiveVoiceAvailability {
    available: true,
    reason: None,
});
engine.start_live_voice(chat_id).await.unwrap();
assert_eq!(engine.session_status(chat_id).unwrap().unwrap().status, SessionStatus::Working);
```

Add an old-OMP fixture advertising `liveVoice: 1` without `liveVoiceSessionContext`; while `Working`, expect `UnsupportedOmp`, but after the Session returns `Idle`, expect ordinary Live to remain available.

- [ ] **Step 2: Run the active-run e2e test and confirm RED**

```bash
cargo test -p zeron-engine --test e2e live_voice_starts_while_omp_run_is_working -- --exact
```

Expected: FAIL with the current active-run precondition.

- [ ] **Step 3: Make the precondition capability-aware**

Remove `LiveVoiceUnavailableReason::ActiveRun` as an ordinary state prohibition and update UI copy/tests that reference it. Determine `busy` from `SessionStatus::Working | SessionStatus::AwaitingInput`; availability is:

```rust
let support = harness.probe_live_voice(Path::new(&cwd)).await?;
let available = support.available && (!busy || support.session_context);
```

Return `UnsupportedOmp` for an old OMP during a busy Session so the action explains that an OMP update is required. Repeat the same authoritative check inside `start_live_voice`; never trust the earlier UI probe.

- [ ] **Step 4: Add latest-value context state to the coordinator**

Create one `tokio::sync::watch` channel per `ActiveLiveVoice`. Expose:

```rust
pub(crate) fn replace_operational_context(&self, call_id: &str, text: String) -> bool;
pub(crate) fn watch_operational_context(
    &self,
    call_id: &str,
) -> Option<watch::Receiver<String>>;
```

`replace_operational_context` must use `send_replace(truncate_live_text(text))`; no await and no dependency on the run's broadcast path.

- [ ] **Step 5: Forward context through the existing Live handle task**

In `attach_live_handle`, add `operational_context.changed()` to the existing `tokio::select!`. On change, send only the newest borrowed value:

```rust
LiveVoiceControl::AppendSessionContext {
    text: operational_context.borrow_and_update().clone(),
}
```

A closed or failed context send may fail the Live call, but must not cancel or alter the coding run.

- [ ] **Step 6: Attach the originating-Chat observer at Live start**

Subscribe before reading the initial snapshot so events cannot fall through the attach race:

```rust
let (_, receiver) = self.subscribe(chat_id, u64::MAX)?;
let status = self
    .session_status(chat_id)
    .map_or(SessionStatus::Idle, |session| session.status);
let messages = self.doc_handle(chat_id)?.watch_messages();
let initial_text = latest_visible_assistant_text(messages.borrow().as_slice());
self.spawn_live_operational_observer(
    call_id.clone(),
    status,
    initial_text,
    receiver,
);
```

The observer owns a `LiveOperationalContext`, applies future events, and calls `replace_operational_context` only when rendered output changes. It exits when the call id no longer matches or the broadcast closes. It must not read `RunJournal::replay`.

- [ ] **Step 7: Prove silence and no run replacement**

Extend e2e assertions after Live starts during `Working`:

- the original run id is unchanged;
- no second user entry appears;
- the fixture receives one `AppendSessionContext` with `Working` and later visible text;
- no delegation is emitted by the context control;
- ending Live leaves the coding run working.

- [ ] **Step 8: Run engine and UI focused tests**

```bash
cargo test -p zeron-engine live_voice -- --nocapture
cargo test -p zeron-engine --test e2e live_voice_starts_while_omp_run_is_working -- --exact
cargo test -p zeron-ui --lib live_voice::tests
```

Expected: PASS.

- [ ] **Step 9: Commit active-run availability and context forwarding**

```bash
git add \
  crates/proto/src/live_voice.rs \
  crates/engine/src/live_voice.rs \
  crates/engine/src/sessions.rs \
  crates/ui/src/live_voice.rs \
  crates/engine/tests/e2e.rs
git commit -m "feat(live): observe active coding runs"
```

---

### Task 6: Route confirmed Live instructions through steer

**Files:**
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/engine/tests/e2e.rs`

**Interfaces:**
- Consumes: existing `SessionsEngine::steer(chat_id, prompt, Some(message_id)) -> SteerOutcome` and Live delegation ownership ids.
- Produces: exactly-once steer with durable fallback when the run settles.

- [ ] **Step 1: Write failing confirmed-steer e2e coverage**

Drive a Live fixture that emits one delegation while an unrelated OMP run is already `Working`. Assert:

```rust
assert!(core.sessions.has_live_run(CHAT));
assert_eq!(harness.received_steers().await, vec!["Change the target to SQLite"]);
assert_eq!(harness.started_runs(), 1);
assert_eq!(
    entries(&core)
        .iter()
        .filter(|entry| entry.role == MessageRole::User)
        .count(),
    2,
    "the original prompt and confirmed voice steer are the only user entries",
);
assert_ne!(
    core.sessions.watch_live_voice().borrow().phase,
    LiveVoicePhase::Idle,
);
```

Add a race fixture that settles immediately before `steer`; expect one fallback run and the same message id, with no duplicate user entry.

- [ ] **Step 2: Run both e2e cases and confirm RED**

```bash
cargo test -p zeron-engine --test e2e live_voice_confirmed_instruction -- --nocapture
```

Expected: FAIL because `handle_live_delegation` always queues a competing run command.

- [ ] **Step 3: Steer before durable fallback**

In `handle_live_delegation`, subscribe to backend events and claim Live ownership before routing. Then:

```rust
let command_id = ownership.command_id;
let message_id = ownership.message_id;
let cancellation = ownership.cancellation;
let outcome = self
    .steer(&chat_id, &run_request.prompt, Some(message_id.clone()))
    .await?;

if outcome == SteerOutcome::NotSteerable {
    host.queue_command_with_id(
        &chat_id,
        command_id,
        SessionCommandPayload::Run {
            request: run_request,
            message_id,
        },
    )?;
}

self.spawn_live_backend_observer(
    call_id.to_owned(),
    delegation_id.to_owned(),
    cancellation,
    receiver,
);
```

Do not call `dispatch` directly for fallback; the existing host executor must preserve command ownership and competing-command rules. The same `message_id` gives deduplication across the settlement race.


- [ ] **Step 4: Run focused steering and continuity tests**

```bash
cargo test -p zeron-engine --test e2e live_voice_confirmed_instruction -- --nocapture
cargo test -p zeron-engine --test e2e live_voice_delegation_is_one_durable_run_with_transient_context -- --exact
```

Expected: PASS, including the existing delegation-continuity regression.

- [ ] **Step 5: Commit steering behavior**

```bash
git add crates/engine/src/sessions.rs crates/engine/tests/e2e.rs
git commit -m "feat(live): steer confirmed voice instructions"
```

---

### Task 7: Update durable contracts and run validation

**Files:**
- Modify: `AGENTS.md`
- Modify: `crates/AGENTS.md`
- Modify: `crates/engine/AGENTS.md`
- Modify: `crates/ui/AGENTS.md` only if its availability ownership text changes
- Modify through archive: `openspec/specs/omp-live-voice/spec.md`
- Modify: `openspec/changes/allow-live-voice-during-active-run/tasks.md`

**Interfaces:**
- Consumes: completed OMP and Comet behavior.
- Produces: synchronized DOX/OpenSpec contracts and verification evidence.

- [ ] **Step 1: Update the nearest DOX owners**

Record only durable rules:

- engine owns the operational observer and latest-value projection;
- active-run Live requires OMP operational-context capability;
- context is display-safe and never reads raw Run Journal payloads;
- confirmed voice instructions steer once with durable fallback;
- UI selection remains a projection and does not own Live teardown.

Remove stale text that says every `Working` or `AwaitingInput` Session makes Live unavailable.

- [ ] **Step 2: Format and run focused/full repository gates**

```bash
cargo fmt --all
cargo test -p zeron-harness
cargo test -p zeron-engine
cargo test -p zeron-ui
cargo build
openspec validate allow-live-voice-during-active-run --strict
```

Expected: every command exits zero.

- [ ] **Step 3: Smoke the actual app surface**

Using the repository's signed development-app flow and OMP source binding:

1. Start one OMP Chat run and wait for visible streaming.
2. Start Live Voice without stopping the run.
3. Remain silent and confirm no assistant audio or new Chat entry occurs.
4. Ask what is happening and confirm the answer reflects a post-activation stream update.
5. Request a direction change; confirm Live asks before sending.
6. Reject once and confirm no Chat entry appears.
7. Repeat, confirm, and verify one user entry and continued identity of the original Session/run.
8. End Live and confirm the coding run remains healthy.

- [ ] **Step 4: Archive and validate OpenSpec**

Mark every completed task in the change, then run:

```bash
openspec archive allow-live-voice-during-active-run --yes
openspec validate --all --strict
```

Expected: the delta is merged into `openspec/specs/omp-live-voice/spec.md` and all specs are valid.

- [ ] **Step 5: Commit contract updates**

```bash
git add \
  AGENTS.md \
  crates/AGENTS.md \
  crates/engine/AGENTS.md \
  crates/ui/AGENTS.md \
  openspec
git commit -m "docs(live): record active-run voice contract"
```

- [ ] **Step 6: Push through the configured gate**

```bash
git push no-mistakes main
```

Expected: the no-mistakes gate validates and updates the fork's `main`; never push to `upstream`.
