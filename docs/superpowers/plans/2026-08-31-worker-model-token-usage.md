# Worker Model and Token Usage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each OMP CLI Worker's effective model and per-model accumulated token usage in the Details Workers widget.

**Architecture:** The bundled OMP lifecycle extension binds the provider conversation ID/path to the URL-addressed Worker Session. A runtime-owned Rust adapter normalizes OMP JSONL into a small durable telemetry marker after lifecycle completion; the disk Host publishes optional additive fields that flow through `WorkersSession` into the existing expandable gpui widget.

**Tech Stack:** Rust 2024, serde/serde_json, vendored Unpeel runtime adapters, OMP extension JavaScript, gpui, OpenSpec.

**Spec:** `docs/plans/2026-08-31-worker-model-token-usage-design.md`

## Global Constraints

- Use OMP JSONL only; never infer model or tokens from terminal text, launch preset, or global config.
- Attribute `usage.totalTokens` once to the effective `provider/model:thinking` identity of each assistant message.
- Keep telemetry device-local and out of Loro, edge sync, Chat Transcript, Chat Transcript Export, and Managed Provider Usage.
- Every new wire field is optional and old/non-OMP Sessions retain the current `command` subtitle.
- A telemetry failure must not fail hook lifecycle, Worker bootstrap, terminal access, or widget rendering.
- Preserve the MIT license and update `third_party/unpeel-upstream.toml` in the same commit as vendored-source changes.
- Follow TDD RED→GREEN and do not push, deploy, tag, or publish.
- Treat every commit command below as an optional checkpoint; run it only after
  the user explicitly authorizes local commits during execution.
- Preserve the pre-existing `CONTEXT.md` modification and stage only task-owned paths.

---

## File Map

- Create `openspec/changes/show-worker-model-token-usage/`: capability contract and execution checklist.
- Create `third_party/unpeel/crates/unpeel-core/src/session_telemetry.rs`: provider-neutral telemetry types, runtime dispatch, atomic marker load/store.
- Create `third_party/unpeel/runtimes/omp/adapter/telemetry.rs`: OMP JSONL validation and normalization.
- Modify `third_party/unpeel/runtimes/_shared/pi-family/assets/lifecycle-extension.js`: publish provider session ID/path with existing lifecycle events.
- Modify `third_party/unpeel/runtimes/_shared/pi-family/adapter/mod.rs`: bind the OMP telemetry reader without changing Prime Agent behavior.
- Modify `third_party/unpeel/crates/unpeel-core/src/integrations/mod.rs`: optional runtime telemetry callback.
- Modify `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`: publish the durable optional telemetry projection.
- Modify `crates/workers-unpeel/src/activity_bridge.rs`: persist hook metadata and refresh telemetry after OMP lifecycle completion.
- Modify `crates/workers-unpeel/src/lib.rs`: additive typed wire/frontier models.
- Modify `crates/ui/src/details_sidebar/{chat_workers,widgets,view}.rs`: projection, formatting, disclosure state, and rendering.
- Modify `third_party/unpeel-upstream.toml`, `third_party/{AGENTS.md,unpeel/AGENTS.md}`, `crates/workers-unpeel/AGENTS.md`, and `crates/ui/AGENTS.md`: provenance and DOX closeout.

---

### Task 1: Authorize the capability in OpenSpec

**Files:**
- Create: `openspec/changes/show-worker-model-token-usage/proposal.md`
- Create: `openspec/changes/show-worker-model-token-usage/design.md`
- Create: `openspec/changes/show-worker-model-token-usage/specs/workers-widget-model-usage/spec.md`
- Create: `openspec/changes/show-worker-model-token-usage/tasks.md`

**Interfaces:**
- Consumes: approved design in `docs/plans/2026-08-31-worker-model-token-usage-design.md`.
- Produces: capability `workers-widget-model-usage` and the scenario IDs used by every later test.

- [ ] **Step 1: Scaffold the change through the CLI**

Run:

```bash
openspec new change "show-worker-model-token-usage"
openspec status --change "show-worker-model-token-usage" --json
```

Expected: the change root and required artifact paths are reported; no change directory is created by hand.

- [ ] **Step 2: Write the proposal and design**

Use these decisions verbatim in the generated artifacts:

```markdown
- OMP provider JSONL is the sole telemetry authority.
- The hook payload binds provider session identity to the Worker Session addressed by the hook URL.
- Token totals are grouped by exact provider/model/thinking identity.
- The Host publishes optional additive telemetry; absence preserves the command subtitle.
- The widget uses the existing stable disclosure state and keeps row-open behavior separate.
```

- [ ] **Step 3: Write the capability scenarios**

Put these scenarios in `specs/workers-widget-model-usage/spec.md`:

```markdown
## ADDED Requirements

### Requirement: A Worker exposes provider-observed model token usage

The Workers widget SHALL show the total tokens attributed to each effective model recorded by an OMP Worker Session, without estimating from terminal text or configuration.

#### Scenario: One OMP model produced the Worker responses
Test: unit — OMP JSONL normalization and Worker row projection.

- **WHEN** an OMP Worker transcript contains assistant messages with provider, model, thinking level, and `usage.totalTokens`
- **THEN** the Worker row shows the normalized model identity and their accumulated token total
- **AND** the session total equals the sum of its per-model totals

#### Scenario: The Worker changes model
Test: unit — ordered OMP model/thinking transitions and multi-model disclosure.

- **WHEN** an OMP Worker changes effective model or thinking level between assistant messages
- **THEN** each message is attributed to the identity active when that message was produced
- **AND** expansion lists the current identity first and every earlier identity with its own total

#### Scenario: Telemetry is unavailable
Test: unit — optional wire decode and UI fallback.

- **WHEN** the provider path is missing, untrusted, malformed, or unsupported
- **THEN** Worker lifecycle and terminal access continue unchanged
- **AND** the row retains its command subtitle without invented model or token values
```

- [ ] **Step 4: Validate and commit the authorization artifact**

Run:

```bash
openspec validate "show-worker-model-token-usage" --type change --strict
git diff --check
```

Expected: validation exits 0.

Commit only the OpenSpec artifacts and approved design:

```bash
git add openspec/changes/show-worker-model-token-usage docs/plans/2026-08-31-worker-model-token-usage-design.md docs/superpowers/plans/2026-08-31-worker-model-token-usage.md
git commit -m "docs(workers): specify worker model token usage"
```

---

### Task 2: Bind OMP provider sessions through the lifecycle hook

**Files:**
- Modify: `third_party/unpeel/runtimes/_shared/pi-family/assets/lifecycle-extension.js`
- Modify: `third_party/unpeel/runtimes/setup_conformance_tests.rs`
- Modify: `crates/workers-unpeel/src/activity_bridge.rs`

**Interfaces:**
- Consumes: hook URL `/hook/<worker-session-id>` and OMP `ctx.sessionManager.getSessionId()/getSessionFile()`.
- Produces: `HookEvent.provider_session_id: Option<String>` and `HookEvent.provider_transcript_path: Option<String>`, persisted through `unpeel_core::session_ops::set_provider_session`.

- [ ] **Step 1: Write RED tests for lifecycle metadata**

Add a package conformance assertion:

```rust
#[test]
fn pi_family_lifecycle_extension_reports_provider_session_identity() {
    assert!(PI_FAMILY_LIFECYCLE_EXTENSION.contains("getSessionId"));
    assert!(PI_FAMILY_LIFECYCLE_EXTENSION.contains("getSessionFile"));
    assert!(PI_FAMILY_LIFECYCLE_EXTENSION.contains("provider_transcript_path"));
}
```

Add an `activity_bridge` request test that posts:

```json
{
  "hook_event_name": "Stop",
  "session_id": "omp-provider-1",
  "provider_transcript_path": "/tmp/omp-provider-1.jsonl"
}
```

and asserts the accepted `HookEvent` keeps the URL Worker id separate from both provider fields.

- [ ] **Step 2: Run the tests and capture RED**

Run:

```bash
cargo test -p zeron-workers-unpeel activity_bridge::tests::hook_provider_identity_is_not_the_worker_identity -- --exact
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml pi_family_lifecycle_extension_reports_provider_session_identity -- --exact
```

Expected: FAIL because the extension and `HookEvent` do not carry provider metadata.

- [ ] **Step 3: Publish metadata from the existing OMP extension**

Implement the payload without sending prompts or message content:

```javascript
function providerSessionMetadata(ctx) {
  const manager = ctx?.sessionManager;
  const sessionId = manager?.getSessionId?.();
  const transcriptPath = manager?.getSessionFile?.();
  return {
    ...(typeof sessionId === "string" && sessionId ? { session_id: sessionId } : {}),
    ...(typeof transcriptPath === "string" && transcriptPath
      ? { provider_transcript_path: transcriptPath }
      : {}),
  };
}

function notify(hookEventName, ctx) {
  const payload = { hook_event_name: hookEventName, ...providerSessionMetadata(ctx) };
  // Keep the existing detached spawn and stdio behavior.
}
```

Pass `ctx` from both `agent_start` and `agent_end` handlers.

- [ ] **Step 4: Persist metadata in the Comet hook ingress**

Extend `HookEvent` with the two optional provider fields. Parse the same aliases accepted by Unpeel's `hook_listener.rs`, then attempt:

```rust
if let Err(error) = unpeel_core::session_ops::set_provider_session(
    &event.session_id,
    event.provider_session_id.as_deref(),
    event.provider_transcript_path.as_deref(),
) {
    tracing::trace!(%error, "could not persist provider session metadata");
}
```

The call uses the URL-derived Worker id and must occur before telemetry refresh. A metadata write failure is traced and does not discard the lifecycle event.

- [ ] **Step 5: Run GREEN tests and commit**

Run:

```bash
cargo test -p zeron-workers-unpeel activity_bridge
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml pi_family_lifecycle_extension_reports_provider_session_identity
git diff --check
```

Commit:

```bash
git add crates/workers-unpeel/src/activity_bridge.rs third_party/unpeel/runtimes/_shared/pi-family/assets/lifecycle-extension.js third_party/unpeel/runtimes/setup_conformance_tests.rs
git commit -m "feat(workers): bind OMP provider sessions"
```

---

### Task 3: Normalize and persist OMP model token telemetry

**Files:**
- Create: `third_party/unpeel/crates/unpeel-core/src/session_telemetry.rs`
- Create: `third_party/unpeel/runtimes/omp/adapter/telemetry.rs`
- Create: `third_party/unpeel/runtimes/omp/fixtures/model-usage.jsonl`
- Modify: `third_party/unpeel/crates/unpeel-core/src/lib.rs`
- Modify: `third_party/unpeel/crates/unpeel-core/src/integrations/mod.rs`
- Modify: `third_party/unpeel/runtimes/omp/adapter/mod.rs`
- Modify: `crates/workers-unpeel/src/activity_bridge.rs`

**Interfaces:**
- Produces: `ModelTokenUsage { model: String, total_tokens: u64, active: bool }` and `SessionTelemetry { total_tokens: u64, models: Vec<ModelTokenUsage> }`.
- Produces: `session_telemetry::refresh(&HostedSessionManifest) -> Result<Option<SessionTelemetry>, String>` and `session_telemetry::load(&str) -> Option<SessionTelemetry>`.
- Consumes: provider marker path written in Task 2.

- [ ] **Step 1: Add the RED OMP fixture**

Create a fixture containing this ordered sequence, one JSON object per line:

```json
{"type":"session","id":"omp-1","cwd":"/tmp/project"}
{"type":"model_change","model":"google-antigravity/gemini-3.7-flash"}
{"type":"thinking_level_change","thinkingLevel":"medium"}
{"type":"message","message":{"role":"assistant","provider":"google-antigravity","model":"gemini-3.7-flash","usage":{"totalTokens":216600}}}
{"type":"model_change","model":"openai-codex/gpt-5.6-sol"}
{"type":"thinking_level_change","thinkingLevel":"high"}
{"type":"message","message":{"role":"assistant","provider":"openai-codex","model":"gpt-5.6-sol","usage":{"totalTokens":42100}}}
{"type":"message","message":{"role":"user","usage":{"totalTokens":999999}}}
not-json
```

- [ ] **Step 2: Write RED parser and persistence tests**

Assert the public result exactly:

```rust
assert_eq!(telemetry.total_tokens, 258_700);
assert_eq!(
    telemetry.models,
    vec![
        ModelTokenUsage {
            model: "openai-codex/gpt-5.6-sol:high".into(),
            total_tokens: 42_100,
            active: true,
        },
        ModelTokenUsage {
            model: "google-antigravity/gemini-3.7-flash:medium".into(),
            total_tokens: 216_600,
            active: false,
        },
    ]
);
```

Add separate tests for saturating `u64` addition, path outside the canonical OMP root, symlink escape, non-JSONL extension, atomic marker replacement, and preservation of the last valid marker when refresh fails.

- [ ] **Step 3: Run the parser tests and capture RED**

Run:

```bash
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml session_telemetry -- --nocapture
```

Expected: FAIL because telemetry dispatch, parser, and marker do not exist.

- [ ] **Step 4: Add the provider-neutral runtime seam**

Define the types with additive serde:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTokenUsage {
    pub model: String,
    pub total_tokens: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTelemetry {
    pub total_tokens: u64,
    pub models: Vec<ModelTokenUsage>,
}
```

Add `ReadSessionTelemetry = fn(&HostedSessionManifest) -> Result<Option<SessionTelemetry>, String>` to `Integration`, a `with_session_telemetry` builder, and provider-neutral dispatch in `session_telemetry.rs`. Store the projection as `session-telemetry.json` through write-temp + rename in the Worker's private Session directory.

- [ ] **Step 5: Implement the OMP parser**

The parser must:

```rust
match value.get("type").and_then(Value::as_str) {
    Some("model_change") => update_model(&value),
    Some("thinking_level_change") => update_thinking(&value),
    Some("message") => accumulate_assistant_usage(&value, &mut totals),
    _ => {}
}
```

For assistant messages, prefer `message.provider + "/" + message.model`; fall back to the latest `model_change`. Append the latest non-empty thinking level. Sum only `message.usage.totalTokens` as `u64`, ignore cost and all user/tool records, then sort current first and remaining identities by descending token total followed by model string.

- [ ] **Step 6: Refresh only at lifecycle boundaries**

After Task 2 persists provider metadata, call `session_telemetry::refresh` when either metadata changed or the accepted hook event is `Stop`. Trace errors without rejecting the hook. Do not call the parser from gpui render or the ordinary bootstrap loop.

- [ ] **Step 7: Run GREEN tests and commit**

Run:

```bash
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml session_telemetry
cargo test -p zeron-workers-unpeel activity_bridge
bun run --cwd "$PWD/third_party/unpeel" validate:runtimes
git diff --check
```

Commit:

```bash
git add third_party/unpeel/crates/unpeel-core/src/session_telemetry.rs third_party/unpeel/crates/unpeel-core/src/lib.rs third_party/unpeel/crates/unpeel-core/src/integrations/mod.rs third_party/unpeel/runtimes/omp/adapter third_party/unpeel/runtimes/omp/fixtures crates/workers-unpeel/src/activity_bridge.rs
git commit -m "feat(workers): collect OMP model token usage"
```

---

### Task 4: Carry optional telemetry through the Host and Workers frontier

**Files:**
- Modify: `third_party/unpeel/crates/unpeel-core/src/controller_host.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/tests/local_bootstrap.rs`

**Interfaces:**
- Consumes: `session_telemetry::load(session_id)` from Task 3.
- Produces: optional wire `totalTokens` and `modelUsage[]`.
- Produces: `WorkersModelTokenUsage` and `WorkersSession::{total_tokens, model_usage}`.

- [ ] **Step 1: Write RED Host and frontier tests**

Extend the disk Host summary test with a telemetry marker and assert:

```rust
assert_eq!(summary["totalTokens"], 258_700);
assert_eq!(summary["modelUsage"][0]["model"], "openai-codex/gpt-5.6-sol:high");
assert_eq!(summary["modelUsage"][0]["active"], true);
```

Add a bootstrap fixture with telemetry and an old fixture without it. The old fixture must decode to `total_tokens == None` and `model_usage.is_empty()`.

- [ ] **Step 2: Run tests and capture RED**

Run:

```bash
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml controller_host::tests::session_summary_includes_durable_model_usage -- --exact
cargo test -p zeron-workers-unpeel --test local_bootstrap
```

Expected: FAIL because the optional fields are not published or decoded.

- [ ] **Step 3: Publish optional Host fields**

After constructing `session_summary`, insert telemetry only when present:

```rust
if let Some(telemetry) = crate::session_telemetry::load(&manifest.session.id) {
    value["totalTokens"] = telemetry.total_tokens.into();
    value["modelUsage"] = serde_json::to_value(telemetry.models).unwrap_or_default();
}
```

Do not emit empty arrays for Sessions without telemetry.

- [ ] **Step 4: Add typed optional frontier fields**

Define:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkersModelTokenUsage {
    pub model: String,
    pub total_tokens: u64,
    pub active: bool,
}
```

Add to `SessionWire` and `WorkersSession`:

```rust
#[serde(default)]
total_tokens: Option<u64>,
#[serde(default)]
model_usage: Vec<WorkersModelTokenUsage>,
```

Preserve them in `From<SessionWire>` and every test constructor.

- [ ] **Step 5: Run GREEN tests and commit**

Run:

```bash
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml controller_host
cargo test -p zeron-workers-unpeel --lib
cargo test -p zeron-workers-unpeel --test local_bootstrap
git diff --check
```

Commit:

```bash
git add third_party/unpeel/crates/unpeel-core/src/controller_host.rs crates/workers-unpeel/src/lib.rs crates/workers-unpeel/tests/local_bootstrap.rs
git commit -m "feat(workers): expose model token telemetry"
```

---

### Task 5: Render Worker model usage with subagent disclosure semantics

**Files:**
- Modify: `crates/ui/src/details_sidebar/chat_workers.rs`
- Modify: `crates/ui/src/details_sidebar/widgets.rs`
- Modify: `crates/ui/src/details_sidebar/view.rs`

**Interfaces:**
- Consumes: `WorkersSession::{total_tokens, model_usage}` from Task 4.
- Produces: `ChatWorkerRow::{total_tokens, model_usage}` and `format_token_total(u64) -> String`.
- Reuses: `ChatWorkersWidgetState::{activity_expanded_with_default,toggle_activity_with_default}` with key `worker:<session_id>`.

- [ ] **Step 1: Write RED projection and formatting tests**

Add exact cases:

```rust
assert_eq!(format_token_total(999), "999 tokens");
assert_eq!(format_token_total(216_600), "216.6k tokens");
assert_eq!(format_token_total(1_820_000), "1.8m tokens");
```

Project a Worker with two model usages and assert current-first ordering is preserved. Project a Worker with no telemetry and assert the subtitle remains `omp`.

- [ ] **Step 2: Write RED disclosure-state tests**

Use the existing expansion map with namespaced ids:

```rust
let key = "worker:worker-1";
assert!(!state.activity_expanded_with_default(key, false));
state.toggle_activity_with_default(key, false);
assert!(state.activity_expanded_with_default(key, false));
```

Assert changing Worker order does not move expansion to another session id.

- [ ] **Step 3: Run focused tests and capture RED**

Run:

```bash
cargo test -p zeron-ui details_sidebar::chat_workers
cargo test -p zeron-ui details_sidebar::widgets
```

Expected: FAIL because Worker rows do not carry telemetry or use disclosure state.

- [ ] **Step 4: Implement the minimal projection and formatter**

Add `total_tokens: Option<u64>` and `model_usage: Vec<WorkersModelTokenUsage>` to `ChatWorkerRow`. Keep formatting pure and shared by collapsed summary and expanded rows:

```rust
pub fn format_token_total(tokens: u64) -> String {
    match tokens {
        0..=999 => format!("{tokens} tokens"),
        1_000..=999_999 => format!("{:.1}k tokens", tokens as f64 / 1_000.0),
        _ => format!("{:.1}m tokens", tokens as f64 / 1_000_000.0),
    }
}
```

- [ ] **Step 5: Render collapsed and expanded Worker telemetry**

For telemetry-bearing rows:

- mount a dedicated chevron hit target keyed by `worker:<session_id>`;
- keep the rest of the row's existing `open_worker_event` click handler;
- show `format_token_total(total_tokens)` in a non-shrinking slot;
- show the active model in a truncating slot;
- when expanded, render every `model_usage` entry with the existing progress-dot geometry and its token total right-aligned;
- retain `worker.command` as the subtitle when telemetry is absent.

Do not create another expansion map or another widget state owner.

- [ ] **Step 6: Run GREEN tests and commit**

Run:

```bash
cargo test -p zeron-ui details_sidebar
cargo test -p zeron-ui chat_export
cargo fmt --all --check
git diff --check
```

Expected: Details tests pass and Chat Transcript Export remains unchanged.

Commit:

```bash
git add crates/ui/src/details_sidebar/chat_workers.rs crates/ui/src/details_sidebar/widgets.rs crates/ui/src/details_sidebar/view.rs
git commit -m "feat(ui): show Worker model token usage"
```

---

### Task 6: DOX, vendored provenance, native QA, and OpenSpec closeout

**Files:**
- Modify: `third_party/unpeel-upstream.toml`
- Modify: `third_party/AGENTS.md`
- Modify: `third_party/unpeel/AGENTS.md`
- Modify: `crates/workers-unpeel/AGENTS.md`
- Modify: `crates/ui/AGENTS.md`
- Modify: `openspec/changes/show-worker-model-token-usage/tasks.md`

**Interfaces:**
- Consumes: completed Tasks 1–5.
- Produces: truthful DOX, vendored tree identity, native visual evidence, and archived OpenSpec capability.

- [ ] **Step 1: Update the nearest DOX owners**

Record these durable contracts:

```markdown
- OMP Worker model usage comes only from its provider-bound JSONL projection.
- Hook URL Session identity and payload provider conversation identity are distinct.
- Worker telemetry is device-local and optional; failure preserves command-only UI.
- Multi-model disclosure reuses the widget's stable activity expansion map.
```

Update Test Coverage Matrices for the new parser, hook integration, frontier decode, and UI projection tests. Do not modify the user's unrelated `CONTEXT.md` hunk.

- [ ] **Step 2: Update vendored provenance from the staged subtree**

Stage only vendored paths, then compute the actual staged subtree id:

```bash
git add third_party/unpeel
tree_id=$(git rev-parse "$(git write-tree):third_party/unpeel")
test -n "$tree_id"
```

Set `vendored_tree` to that exact value and increment `local_modifications_count` only if the repository's provenance convention counts this logical patch as a new local modification. Re-stage `third_party/unpeel-upstream.toml` and verify the stored value equals the computed value.

- [ ] **Step 3: Run static and focused gates**

Run:

```bash
bun run --cwd "$PWD/third_party/unpeel" validate:runtimes
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml session_telemetry
cargo test --manifest-path third_party/unpeel/crates/Cargo.toml controller_host
cargo test -p zeron-workers-unpeel
cargo test -p zeron-ui details_sidebar
cargo test -p zeron-ui chat_export
cargo fmt --all --check
cargo check --workspace
cargo build -p zeron
openspec validate "show-worker-model-token-usage" --type change --strict
git diff --check
```

Expected: every command exits 0.

- [ ] **Step 4: Run native gpui acceptance**

Launch the rebuilt app against a real OMP Worker and prove with physical UI interaction:

1. Before the first completed assistant response, the row shows the existing `omp` fallback.
2. After `agent_end`, the row refreshes without reopening Details and shows the exact provider/model/thinking identity plus accumulated tokens.
3. Expanding a one-model Worker shows one model row with the same total.
4. Switching model and completing another response shows two correctly attributed rows, current first, and the session total equals their sum.
5. Narrowing Details truncates model identity while token text and lifecycle status remain visible.
6. Clicking the chevron toggles telemetry; clicking the remaining row opens the same Worker terminal.

Capture screenshots of collapsed, expanded, multi-model, and narrow-width states. Static tests do not substitute for this gate.

- [ ] **Step 5: Complete and archive OpenSpec**

Mark every verified task complete, then run:

```bash
openspec validate "show-worker-model-token-usage" --type change --strict
openspec archive "show-worker-model-token-usage"
openspec validate --all --strict
```

Expected: the change is archived and the capability spec remains valid.

- [ ] **Step 6: Review the final staged scope and commit**

Run:

```bash
git status --short
git diff --cached --stat
git diff --cached --check
git diff --cached --name-only | rg '^CONTEXT\.md$' && exit 1 || true
```

Expected: only Task 6 documentation/provenance/OpenSpec paths are staged; `CONTEXT.md` remains unstaged and untouched.

Commit:

```bash
git add third_party/unpeel-upstream.toml third_party/AGENTS.md third_party/unpeel/AGENTS.md crates/workers-unpeel/AGENTS.md crates/ui/AGENTS.md openspec
git commit -m "docs(workers): close model usage capability"
```

Do not push.

---

## Plan Self-Review

- **Spec coverage:** D1–D8 map to Tasks 2–6; every failure path has a test or native acceptance gate.
- **Type consistency:** `ModelTokenUsage`/`SessionTelemetry` are the core types; `WorkersModelTokenUsage` is the frontier DTO; `ChatWorkerRow` carries the same optional fields into gpui.
- **Scope:** OMP only; no Managed Provider Usage, export, model selection, CRDT, edge, or unrelated runtime work.
- **TDD:** every implementation task begins with a named RED test and ends with focused GREEN gates plus an independently reviewable commit.
