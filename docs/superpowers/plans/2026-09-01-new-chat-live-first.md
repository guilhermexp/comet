# New Chat Live-First Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Live Voice available as the first action on an eligible new-Chat canvas, equivalent to launching a fresh OMP process and entering `/live`.

**Architecture:** The composer derives a draft Live affordance from the selected device and OMP configuration. On click it resolves the same Checkout and `ChatConfig` used by the first text send, materializes the Chat, and calls the existing engine `StartLiveVoice` RPC without leaving the draft. It selects the Chat only after Live starts; failure removes the untouched Chat and worktree without disrupting another selected Chat or Live call.

**Tech Stack:** Rust, gpui, serde_json, existing Zeron RPC client, cargo test.

**Spec:** `openspec/changes/add-omp-live-voice/specs/omp-live-voice/spec.md`

## Global Constraints

- No audio or casual Live transcript may enter Comet RPC, CRDT, uploads, or logs.
- Engine `StartLiveVoice` remains the authoritative capability and eligibility gate.
- Existing selected-Chat Live behavior and first-text Chat creation remain unchanged.
- A failed voice-first attempt must not leave an empty Chat or worktree.
- Use the selected device, project, Checkout, and resolved OMP `ChatConfig`.

---

### Task 1: New-Chat Live Affordance

**Files:**
- Modify: `crates/ui/src/live_voice.rs`
- Modify: `crates/ui/src/composer.rs`

**Interfaces:**
- Consumes: `Pickers::resolved`, `AppState::effective_device_id`, and `AppState::local_device_id`.
- Produces: draft availability through the existing `LiveVoiceViewModel::derive` contract and a composer draft eligibility helper.

- [x] **Step 1: Write the failing view-model test**

Add a test that derives the model with no selected Chat, an available draft result, and idle Live state. Assert `show_microphone`, `microphone_enabled`, and `microphone_tooltip == "Start Live Voice"`.

- [x] **Step 2: Run the focused test and confirm it fails**

Run: `cargo test -p zeron-ui live_voice_new_chat -- --nocapture`

Expected: FAIL because the current model hides the microphone whenever `selected_chat_id` is `None`.

- [x] **Step 3: Implement the minimal affordance**

Allow an available draft result to show the microphone without a selected Chat. In `Composer::live_voice_model`, synthesize that result only when the effective device is local, the resolved harness is `HarnessId::Omp`, and the engine is connected; render an in-flight start as disabled with `Starting Live Voice…`.

- [x] **Step 4: Run the focused test**

Run: `cargo test -p zeron-ui live_voice_new_chat -- --nocapture`

Expected: PASS.

---

### Task 2: Voice-First Chat Materialization

**Files:**
- Modify: `crates/ui/src/composer.rs`
- Modify: `openspec/changes/add-omp-live-voice/tasks.md`

**Interfaces:**
- Consumes: `CheckoutPlan`, `ResolvedRunConfig::chat_config`, `methods::CREATE_WORKTREE`, `methods::MUTATE`, and `methods::START_LIVE_VOICE`.
- Produces: a pure new-Chat launch plan, a shared `createChat` mutation builder used by text and Live, and `Composer::start_live_voice`.

- [x] **Step 1: Write failing pure planning tests**

Cover current checkout, existing worktree, and new worktree. Assert the plan records the expected cwd and branch; for a fresh worktree, assert it records the repo path and base ref needed by `CreateWorktree`. Cover local-device OMP eligibility and reject remote or non-OMP drafts.

- [x] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test -p zeron-ui live_voice_new_chat -- --nocapture`

Expected: FAIL because no voice-first launch plan or materialization flow exists.

- [x] **Step 3: Extract the shared Chat mutation builder**

Build `{ "op": "createChat", "chatId": ... }`, add either `spaceId` or `deviceId`, and add resolved `cwd`, `branch`, and serialized `config`. Replace the text-send inline JSON construction with this helper without changing its order or failure semantics.

- [x] **Step 4: Implement voice-first start**

When no Chat is selected, mint a Chat id and capture the picker plan. If a fresh worktree is selected, call `CreateWorktree` with a client deadline above the engine deadline and retain raw cleanup identity. Call `Mutate createChat`, then `StartLiveVoice` without a shorter client timeout, and select the new Chat only if the draft is still current. While this future is active, disable the microphone. On failure, remove the untouched Chat and any created worktree; stop Live only when this attempt is known to have started it.

- [x] **Step 5: Run focused UI tests**

Run: `cargo test -p zeron-ui live_voice -- --nocapture`

Expected: all Live Voice UI tests pass.

- [x] **Step 6: Build the desktop binary**

Run: `cargo build -p zeron`

Expected: successful build.

- [x] **Step 7: Mark the OpenSpec task complete**

Change C7a to `[x]` only after the focused tests and build pass.

- [x] **Step 8: Commit the completed change**

Commit the specification, plan, tests, implementation, and task update together with message `fix(live): allow voice-first new chats`.
