# Live Voice Shared OMP Session Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Comet Live Voice resume or create the Chat's native OMP session so voice and text share one session identity while delegated coding work remains owned by Comet's durable run pipeline.

**Architecture:** `SessionsEngine` resolves the Chat's current OMP session before Live startup. `OmpHarness` launches a normal RPC child, optionally switches to the existing session, reads the effective session identity, then starts Live in host-delegation mode. The returned identity is persisted on the Chat before Live is exposed as active; the Live child remains a media/conversation frontend and the normal backend child remains the only coding-session writer.

**Tech Stack:** Rust 2024, Tokio, serde_json, OMP RPC UI protocol v2, Automerge-backed Chat metadata, OpenSpec.

**Spec:** `docs/plans/2026-08-31-omp-live-voice-design.md`

## Global Constraints

- Voice and text MUST use the same native OMP session identity for one Chat.
- A Chat without an OMP session MUST create and persist one during successful Live startup.
- Live MUST remain `delegationMode: "host"`; it MUST NOT execute tools or persist casual voice transcripts.
- Delegated work MUST continue through exactly one durable `SessionCommandPayload::Run` and the existing `SessionsEngine` fold.
- Existing Live eligibility, preemption, idempotent stop, local-only RPC, and OMP-owned media boundaries MUST remain unchanged.
- Explicit `OMP_EXECUTABLE` overrides MUST continue to win over Cargo's development launcher.
- TDD is mandatory: observe each focused test fail before implementation, then pass.

---

### Task 1: Align OpenSpec with shared session ownership

**Files:**
- Modify: `openspec/changes/add-omp-live-voice/design.md`
- Modify: `openspec/changes/add-omp-live-voice/specs/omp-live-voice/spec.md`
- Modify: `openspec/changes/add-omp-live-voice/tasks.md`

**Interfaces:**
- Consumes: approved design in `docs/plans/2026-08-31-omp-live-voice-design.md`.
- Produces: normative requirements for session resume, new-session identity persistence, and single-writer host delegation.

- [ ] **Step 1: Replace the old ephemeral-session decision**

Change D-01 and the architecture description so the Live child resumes the Chat OMP session or creates the first one, while host-mode Live remains non-writing. Preserve D-03 durable delegation and D-04 authoritative backend folding.

- [ ] **Step 2: Add normative session continuity scenarios**

Add requirements equivalent to:

```markdown
### Requirement: Live shares the Chat OMP session

The system SHALL start Live Voice with the Chat's stored OMP session identity when one exists and SHALL persist the effective OMP session identity when Live creates the first session for a Chat.

#### Scenario: Existing OMP session is resumed
- **WHEN** Live starts for a Chat with a stored OMP session in the same Checkout
- **THEN** the Live child SHALL switch to that session before `live_start`
- **AND** subsequent text runs SHALL resume the same identity

#### Scenario: Live creates the first OMP session
- **WHEN** Live starts for an eligible Chat without a stored OMP session
- **THEN** OMP SHALL create a normal session
- **AND** Comet SHALL persist its non-empty identity before exposing Live as active
```

- [ ] **Step 3: Update C3/C4/C8 audit language**

C3 must name session switch and identity return. C4 must name engine persistence. C8 remains incomplete until the real voice/session smoke passes.

- [ ] **Step 4: Validate the change**

Run:

```bash
openspec validate add-omp-live-voice --strict --no-interactive
```

Expected: `Change 'add-omp-live-voice' is valid`.

- [ ] **Step 5: Commit the spec amendment**

```bash
git add openspec/changes/add-omp-live-voice
git commit -m "spec(live): share the Chat OMP session"
```

---

### Task 2: Return a session-bound OMP Live handle

**Files:**
- Modify: `crates/harness/src/lib.rs:71-113`
- Modify: `crates/harness/src/omp/mod.rs:318-363`
- Modify: `crates/harness/tests/omp_rpc.rs:330-510`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh:58-220`

**Interfaces:**
- Consumes: existing OMP commands `switch_session`, `get_state`, and `live_start`.
- Produces:

```rust
pub struct LiveVoiceRequest {
    pub cwd: String,
    pub resume: Option<String>,
}

pub struct LiveVoiceHandle {
    pub session_id: String,
    pub events: BoxStream<'static, Result<LiveVoiceEvent, HarnessError>>,
    pub controls: mpsc::Sender<LiveVoiceControl>,
}
```

- [ ] **Step 1: Write failing harness tests**

Update the Live fixture tests to assert:

```rust
let handle = harness
    .start_live_voice(LiveVoiceRequest {
        cwd: temp.path().to_string_lossy().into_owned(),
        resume: Some("/tmp/omp-session.jsonl".into()),
    })
    .await
    .unwrap();
assert_eq!(handle.session_id, "s-1");
```

Add a second test with `resume: None` and assert `session_id == "s-1"`. Change the fake fixture so Live scenarios reject `--no-session`, accept the expected `switch_session`, require `get_state` before `live_start`, and keep rejecting model, thinking, host-tool, and subagent-subscription setup.

- [ ] **Step 2: Run focused tests and observe failure**

Run:

```bash
cargo test -p zeron-harness omp_live_frontend -- --nocapture
```

Expected: compile failure because `LiveVoiceRequest::resume` and `LiveVoiceHandle::session_id` do not exist, or fixture failure because the Live child still receives `--no-session` and omits session setup.

- [ ] **Step 3: Extend the shared harness contract**

Add `resume: Option<String>` to `LiveVoiceRequest` and `session_id: String` to `LiveVoiceHandle`. Update the unsupported default test and every mock constructor with explicit values; do not add defaults that hide missing session identity.

- [ ] **Step 4: Make OMP Live session-bound**

In `OmpHarness::start_live_voice`:

```rust
let process = OmpProcess::start(self.launch(PathBuf::from(&request.cwd), false, None)?).await?;
if !process.capabilities().live_voice {
    let _ = process.shutdown().await;
    return Err(HarnessError::Unsupported(
        "installed OMP does not support Live Voice; update OMP".into(),
    ));
}
if let Some(session_path) = request.resume.as_deref() {
    let response = process
        .request(json!({ "type": "switch_session", "sessionPath": session_path }))
        .await?;
    if response.get("cancelled").and_then(Value::as_bool) == Some(true) {
        let _ = process.shutdown().await;
        return Err(HarnessError::Protocol(
            "OMP Live session resume was cancelled".into(),
        ));
    }
}
let state = process.request(json!({ "type": "get_state" })).await?;
let session_id = state_session_id(&state).ok_or_else(|| {
    HarnessError::Protocol("OMP Live state omitted its session identity".into())
})?;
```

Then preserve the existing subscribe-before-`live_start` ordering, start host-mode Live, and return `LiveVoiceHandle { session_id, events, controls }`. Every failure after process creation must shut down the child before returning.

- [ ] **Step 5: Run harness tests**

Run:

```bash
cargo test -p zeron-harness omp_live_frontend -- --nocapture
cargo test -p zeron-harness --test omp_rpc full_rpc_run_normalizes_events_and_resumes_session
```

Expected: all selected tests pass. The normal run test proves the backend resume path did not change.

- [ ] **Step 6: Commit the harness contract**

```bash
git add crates/harness/src/lib.rs crates/harness/src/omp/mod.rs crates/harness/tests/omp_rpc.rs crates/harness/tests/fixtures/fake-omp-rpc.sh
git commit -m "feat(omp): bind Live to the Chat session"
```

---

### Task 3: Persist Live's effective session on the Chat

**Files:**
- Modify: `crates/engine/src/sessions.rs:377-437`
- Modify: `crates/engine/src/live_voice.rs:700-790`
- Modify: `crates/engine/tests/e2e.rs:180-260`

**Interfaces:**
- Consumes: `LiveVoiceRequest { cwd, resume }` and `LiveVoiceHandle::session_id` from Task 2.
- Produces: `SessionsEngine::start_live_voice` resolves the current Chat session and persists the effective identity through existing `Inner::remember_harness_session(chat_id, session_id, cwd)`.

- [ ] **Step 1: Extend the mock harness and write failing engine tests**

Make the fake Live harness record each `LiveVoiceRequest` and return a configurable `session_id`. Add tests for both paths:

```rust
core.sessions.start_live_voice("live-a").await.unwrap();
let request = live_harness.last_request().unwrap();
assert_eq!(request.resume.as_deref(), Some("existing-omp-session"));
```

```rust
core.sessions.start_live_voice("live-new").await.unwrap();
assert_eq!(workspace.chat_harness_session("live-new").unwrap().0, "created-by-live");
```

Then dispatch the next normal text command and assert its recorded `RunRequest.resume` is `Some("created-by-live")`.

- [ ] **Step 2: Run focused engine tests and observe failure**

Run:

```bash
cargo test -p zeron-engine live_voice_session -- --nocapture
```

Expected: compile or assertion failure because Live startup neither supplies `resume` nor stores the returned identity.

- [ ] **Step 3: Inject and persist session identity**

In `SessionsEngine::start_live_voice`, after preconditions and before harness start:

```rust
let resume = self.inner.resume_for(chat_id, &cwd);
let handle = harness
    .start_live_voice(LiveVoiceRequest {
        cwd: cwd.clone(),
        resume,
    })
    .await?;
self.inner
    .remember_harness_session(chat_id, &handle.session_id, &cwd);
```

Keep the existing reservation/cancellation checks and error transitions. Persist only after the harness has returned a successfully started handle with a non-empty identity. Then attach the handle exactly as before.

- [ ] **Step 4: Update all fake handle constructors**

Every test `LiveVoiceHandle` must supply a meaningful `session_id`. Do not use an empty string; the production harness rejects missing identity.

- [ ] **Step 5: Run engine regression tests**

Run:

```bash
cargo test -p zeron-engine live_voice -- --nocapture
cargo test -p zeron-engine --test e2e deterministic_queue_command_id_is_returned_and_executes_once
```

Expected: Live lifecycle/delegation/session tests and the existing durable queue test pass.

- [ ] **Step 6: Commit engine session persistence**

```bash
git add crates/engine/src/sessions.rs crates/engine/src/live_voice.rs crates/engine/tests/e2e.rs
git commit -m "feat(engine): persist Live OMP session identity"
```

---

### Task 4: Verify shared-session behavior and close the change

**Files:**
- Modify: `openspec/changes/add-omp-live-voice/tasks.md`
- Existing packaging change: `dist/macos/Info.plist`

**Interfaces:**
- Consumes: session-bound harness and engine behavior from Tasks 2-3.
- Produces: validated OpenSpec audit state and signed package evidence.

- [ ] **Step 1: Format and restore unrelated formatter spill**

Run:

```bash
cargo fmt --all
git checkout -- crates/engine/src/lib.rs crates/harness/src/omp/process.rs
```

Restore only files whose changes are formatter-only and unrelated; do not discard intentional fixture/test changes.

- [ ] **Step 2: Run targeted gates**

```bash
cargo test -p zeron-proto live_voice
cargo test -p zeron-harness omp_live
cargo test -p zeron-engine live_voice
cargo test -p zeron-ui live_voice
cargo test -p zeron-rpc
cargo build -p zeron
openspec validate add-omp-live-voice --strict --no-interactive
```

Expected: all commands pass.

- [ ] **Step 3: Run Clippy and classify only pre-existing failures**

```bash
cargo clippy -p zeron-harness -p zeron-engine -p zeron-ui --all-targets -- -D warnings
```

Expected for this branch: the repository-wide gate may still fail on pre-existing warnings in `zeron-theme`, `zeron-doc`, `zeron-harness`, and unrelated UI files. Confirm no finding names the Live session changes; do not widen scope into unrelated lint cleanup.

- [ ] **Step 4: Package and verify the signed app**

```bash
CODESIGN_IDENTITY="Apple Development: guilhermehenriquevarela@gmail.com (VBGAFQN569)" scripts/package-macos.sh
codesign --verify --deep --strict --verbose=2 target/package/Zeron.app
plutil -extract NSMicrophoneUsageDescription raw target/package/Zeron.app/Contents/Info.plist
```

Expected: valid signature and the exact microphone purpose string.

- [ ] **Step 5: Perform the real shared-session smoke**

From a supported local OMP Chat with an existing session:

1. start Live and verify the button is enabled;
2. ask a context question whose answer depends on the existing OMP session;
3. request one repository operation and verify one durable command/tools/final;
4. end Live, send a text follow-up, and verify OMP resumes the same session identity;
5. repeat on a new Chat and verify Live creates the first session used by the next text turn;
6. verify mute/unmute, Chat switch, and quit cleanup.

- [ ] **Step 6: Review the complete diff**

Run the repository's CodeRabbit review over the feature base through HEAD. Resolve every correctness finding in scope, rerun the focused test that covers each fix, and leave unrelated baseline findings documented.

- [ ] **Step 7: Record audit evidence and validate all OpenSpec changes**

Mark C1-C7 complete from existing test evidence. Mark C8 complete only after the real shared-session smoke succeeds. Then run:

```bash
openspec validate --all --strict
```

- [ ] **Step 8: Commit closure metadata**

```bash
git add dist/macos/Info.plist openspec/changes/add-omp-live-voice/tasks.md
git commit -m "chore(macos): enable OMP live microphone access"
```
