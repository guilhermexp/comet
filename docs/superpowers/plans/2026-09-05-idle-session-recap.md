# Idle Session Recap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement idle session recap with exact orchestrator.dev parity in Comet.

**Architecture:** A typed RPC `GenerateChatRecap` on the local engine host executes a one-shot throwaway prompt via the chat's harness cheapest model on the stored transcript tail. A pure UI policy (`evaluate_idle_recap`) arms a timer when the chat sits idle without streaming/compaction/draft, and recaps are persisted in `DetailsSidebarPreferences` (`ui-settings.json`) and rendered in the Workspace widget under "Projects worked".

**Tech Stack:** Rust edition 2024, GPUI, Tokio, Serde, Zeron RPC, Zeron Engine.

**Spec:** `docs/plans/2026-09-05-idle-session-recap-design.md` and `openspec/changes/add-idle-session-recap/`

## Global Constraints

- Never touch or mutate the live session CRDT document; recaps are strictly derived metadata.
- Zero extra turns or context consumption on the chat session.
- Epoch guard: `epoch == transcript.len()`. Stale recaps are invalidated immediately on turn start or draft typing.
- Recaps survive application restarts via `ui-settings.json`, with max 50 entries and 24h retention.
- All code must pass `cargo fmt --all` and workspace tests.

---

### Task 1: RPC Protocol Definition

**Files:**
- Modify: `crates/rpc/src/method.rs`
- Test: `crates/rpc/tests` or compile check

**Interfaces:**
- Produces: `GenerateChatRecapParams`, `GenerateChatRecapReply`, `GENERATE_CHAT_RECAP`, `GenerateChatRecap`

- [ ] **Step 1: Declare the wire params and reply types in `crates/rpc/src/method.rs`**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateChatRecapParams {
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateChatRecapReply {
    pub recap: Option<String>,
}
```

- [ ] **Step 2: Add `GENERATE_CHAT_RECAP` to the `rpc_methods!` macro in `crates/rpc/src/method.rs`**

```rust
GENERATE_CHAT_RECAP / GenerateChatRecap = "GenerateChatRecap" {
    params: GenerateChatRecapParams,
    reply: GenerateChatRecapReply,
    local_only: true,
    deadline_secs: 45,
}
```

- [ ] **Step 3: Run `cargo check -p zeron-rpc` to verify compilation**

Run: `cargo check -p zeron-rpc`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/rpc/src/method.rs
git commit -m "feat(rpc): add GenerateChatRecap method"
```

---

### Task 2: Engine Recap Pipeline & Unit Tests

**Files:**
- Create: `crates/engine/src/recap.rs`
- Modify: `crates/engine/src/lib.rs`
- Modify: `crates/engine/src/rpc.rs`

**Interfaces:**
- Consumes: `HarnessRegistry`, `WorkspaceHost`, `SessionDoc`, `zeron_harness::Harness`
- Produces: `select_recap_transcript`, `build_recap_prompt`, `validate_recap`, `run_recap_model`

- [ ] **Step 1: Write the failing tests for transcript selection, prompt building, and validation**

Create unit tests in `crates/engine/src/recap.rs`:
- `select_recap_transcript_bounds`: keeps up to 40 messages, 6000 chars total, 800 chars per message, skips tool parts, preserves user/assistant order.
- `build_recap_prompt_structure`: checks goal anchor inclusion, language match instruction, word count directive.
- `validate_recap_cleaning`: removes preambles like "Sure:", strips markdown asterisks/backticks/quotes, clamps to 280 chars.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p zeron-engine recap`
Expected: FAIL (module not implemented)

- [ ] **Step 3: Implement `select_recap_transcript`, `build_recap_prompt`, `validate_recap`, and `run_recap_model` in `crates/engine/src/recap.rs`**

Implement pure functions and harness invocation logic matching `orchestrator.dev`.
Wire module in `crates/engine/src/lib.rs`.

- [ ] **Step 4: Wire RPC handler in `crates/engine/src/rpc.rs`**

Handle `methods::GENERATE_CHAT_RECAP`:
- Load chat and transcript from doc host.
- Call `run_recap_model`.
- Return `GenerateChatRecapReply { recap }`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p zeron-engine recap`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/recap.rs crates/engine/src/lib.rs crates/engine/src/rpc.rs
git commit -m "feat(engine): implement GenerateChatRecap pipeline and one-shot execution"
```

---

### Task 3: UI Idle Recap Policy, Persistence & Preferences

**Files:**
- Create: `crates/ui/src/details_sidebar/idle_recap.rs`
- Modify: `crates/ui/src/details_sidebar/mod.rs`
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/src/settings.rs`

**Interfaces:**
- Produces: `IdleRecapEntry`, `IdleRecapState`, `IdleRecapAction`, `evaluate_idle_recap`, `prune_idle_recaps`
- Consumes: `DetailsSidebarPreferences`, `SettingsStore`

- [ ] **Step 1: Write failing unit tests for `evaluate_idle_recap` and `prune_idle_recaps`**

In `crates/ui/src/details_sidebar/idle_recap.rs`:
- Test all branches: feature off, streaming, compacting, draft, epoch match (keep), messageCount 0 (keep), delay clamping.
- Test retention: drop entries > 24h old, drop future timestamps, cap at 50 entries.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p zeron-ui idle_recap`
Expected: FAIL

- [ ] **Step 3: Implement pure policy and retention in `crates/ui/src/details_sidebar/idle_recap.rs`**

Implement `evaluate_idle_recap` and `prune_idle_recaps`.
Expose module in `crates/ui/src/details_sidebar/mod.rs`.

- [ ] **Step 4: Update `DetailsSidebarPreferences` in `crates/ui/src/details_sidebar/view.rs`**

Add:
```rust
#[serde(default)]
pub idle_recaps: HashMap<String, IdleRecapEntry>,
#[serde(default = "default_true")]
pub idle_recap_enabled: bool,
#[serde(default = "default_recap_delay")]
pub idle_recap_delay_seconds: u64,
```
Apply `prune_idle_recaps` on load and update.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p zeron-ui idle_recap`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/ui/src/details_sidebar/idle_recap.rs crates/ui/src/details_sidebar/mod.rs crates/ui/src/details_sidebar/view.rs crates/ui/src/settings.rs
git commit -m "feat(ui): implement idle recap policy, retention, and preferences persistence"
```

---

### Task 4: UI Timer Lifecycle & Widget Rendering

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`

**Interfaces:**
- Consumes: `evaluate_idle_recap`, `RpcClient::call_typed::<GenerateChatRecap>`, `Theme`
- Produces: Rendered `※ recap:` row in Workspace card below "Projects worked"

- [ ] **Step 1: Add idle timer management in `DetailsSidebarState` / active chat view**

When context or transcript updates:
- Evaluate `evaluate_idle_recap`.
- Handle `Clear`, `Keep`, `Arm`.
- If `Arm`: spawn debounced task using `cx.spawn`.
- When timer fires: re-check idle conditions, dispatch `client.call_typed::<GenerateChatRecap>`, and on success update `idle_recaps` in preferences.

- [ ] **Step 2: Render recap in Workspace card in `crates/ui/src/details_sidebar/view.rs`**

Inside `workspace_body`, below `worked_section`:
Render the formatted recap line:
`※ recap: <text>` and formatted clock timestamp.

- [ ] **Step 3: Run `cargo check -p zeron-ui` and unit tests**

Run: `cargo check -p zeron-ui && cargo test -p zeron-ui`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/ui/src/details_sidebar/view.rs
git commit -m "feat(ui): wire idle recap timer and render in Details Workspace widget"
```

---

### Task 5: Formatting, Full Suite & OpenSpec Validation

**Files:**
- Touch: all modified files

- [ ] **Step 1: Format codebase**

Run: `cargo fmt --all`

- [ ] **Step 2: Run workspace test suite**

Run: `cargo test -p zeron-rpc -p zeron-engine -p zeron-ui`
Expected: ALL PASS

- [ ] **Step 3: Validate OpenSpec change**

Run: `openspec validate add-idle-session-recap --strict --no-interactive`
Expected: `Change 'add-idle-session-recap' is valid`

- [ ] **Step 4: Commit final changes**

```bash
git add -u
git commit -m "chore: format and finalize idle session recap implementation"
```
