# Orchestrator Streaming and Tool Block Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Comet's native transcript honest live tool states, Orchestrator-style command blocks, optional execution metadata, and a persisted Thinking/Thought surface across all runtimes.

**Architecture:** Preserve the existing `AgentEvent -> MessagePart -> Row -> GPUI` pipeline. Add only optional execution fields and a reasoning part to the protocol/doc schema, centralize lifecycle copy in `zeron_proto::view`, and extend the existing transcript fold/detail renderer rather than adding a parallel component tree.

**Tech Stack:** Rust 2024, serde, Loro documents, GPUI, pulldown-cmark incremental renderer, existing motion/loaders/theme/tool-icon modules, Cargo tests.

**Spec:** `docs/plans/2026-08-21-orchestrator-streaming-tool-block-parity-design.md`

## Global Constraints

- Preserve all existing runtimes, including Pi, Claude, Codex, Cursor, ACP, and OMP.
- New wire and document fields must be optional and backward compatible.
- Keep `ToolCall -> ToolResult` as the universal lifecycle; do not add universal output-delta events.
- Do not fabricate stdout/stderr separation or execution metadata a runtime does not report.
- Keep output behind the existing bounded-summary and sidecar privacy policy.
- Preserve transcript row identity, analytic height calculations, virtualization, and user fold overrides.
- Reuse the existing contextual tool icons, GPUI loaders, theme, Markdown renderer, and changes renderer.
- Use TDD and commit each task locally. Do not push, release, or promote to `main`.

---

## File Structure

- Modify `crates/proto/src/agent.rs`: additive `ToolExecutionMeta` and optional `ToolResult.execution`.
- Modify `crates/proto/src/view.rs`: pure lifecycle presentation descriptor.
- Modify `crates/harness/src/codex/normalize.rs`: preserve aggregated output, exit code, and duration.
- Modify `crates/harness/src/claude/normalize.rs`: preserve safe tool-result text.
- Modify `crates/harness/src/omp/normalize.rs`: preserve execution metadata when reported.
- Modify `crates/harness/src/acp/normalize.rs`: preserve execution metadata when reported.
- Modify `crates/harness/src/cursor/mod.rs`: populate the additive field with `None`.
- Modify `crates/doc/src/parts.rs`: execution metadata, reasoning part, folding and summaries.
- Modify `crates/doc/src/schema.rs`: additive execution/reasoning Loro shape and salvage.
- Modify `crates/doc/src/rebuild.rs`: carry new fields through rebuild.
- Modify `crates/engine/src/sessions.rs`: render/privacy projection and reasoning completion boundary.
- Modify `crates/ui/src/transcript.rs`: lifecycle status, active-detail defaults, role-specific output tones, reasoning row/rendering, fingerprints, and tests.
- Reuse `crates/ui/src/loaders.rs`, `crates/ui/src/markdown/*`, `crates/ui/src/theme.rs`, and `crates/ui/src/tool_icons.rs` unchanged unless a focused test proves a small shared helper is needed.

### Task 1: Shared tool lifecycle presentation

**Files:**
- Modify: `crates/proto/src/view.rs`
- Test: `crates/proto/src/view.rs`

**Interfaces:**
- Consumes: `ToolCall`, `resolved: bool`, `is_error: bool`.
- Produces: `ToolOutcome`, `ToolPresentation`, and `tool_presentation(&ToolCall, bool, bool)`.

- [ ] **Step 1: Write failing presentation tests**

Add tests that require the exact lifecycle copy:

```rust
#[test]
fn exec_presentation_tracks_running_success_and_failure() {
    let call = ToolCall::Exec { command: "cargo test".into() };
    let running = tool_presentation(&call, false, false);
    assert_eq!(running.label, "Running command");
    assert_eq!(running.outcome, ToolOutcome::Pending);

    let success = tool_presentation(&call, true, false);
    assert_eq!(success.label, "Ran command");
    assert_eq!(success.outcome, ToolOutcome::Success);

    let failed = tool_presentation(&call, true, true);
    assert_eq!(failed.label, "Ran command");
    assert_eq!(failed.outcome, ToolOutcome::Failed);
}

#[test]
fn non_exec_tools_use_active_and_completed_verbs_without_success_badges() {
    let read = ToolCall::ReadFile { path: "src/lib.rs".into() };
    assert_eq!(tool_presentation(&read, false, false).label, "Reading");
    assert_eq!(tool_presentation(&read, true, false).label, "Read");
    assert!(!tool_presentation(&read, true, false).show_outcome_label);
}
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cargo test -p zeron-proto --lib tool_presentation
```

Expected: compile failure because the presentation types/functions do not exist.

- [ ] **Step 3: Implement the pure descriptor**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutcome { Pending, Success, Failed }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPresentation {
    pub label: &'static str,
    pub detail: String,
    pub outcome: ToolOutcome,
    pub show_outcome_label: bool,
}

pub fn tool_presentation(call: &ToolCall, resolved: bool, is_error: bool) -> ToolPresentation
```

Reuse `tool_chip_content_raw` for detail, normalize it with `single_line`, and map bounded active/completed verbs per `ToolCall` variant. Only `Exec` sets `show_outcome_label = resolved`.

- [ ] **Step 4: Run focused and full proto tests**

```bash
cargo test -p zeron-proto --lib tool_presentation
cargo test -p zeron-proto --lib
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/proto/src/view.rs
git commit -m "feat(proto): describe live tool lifecycle"
```

### Task 2: GPUI command progress, outcome, and disclosure behavior

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `zeron_proto::view::tool_presentation`, existing `ToolItem.resolved/is_error`, group `auto_open`, and fold maps.
- Produces: fixed-footprint pending spinner, command outcome trail, per-tool derived default, and role-aware detail rendering.

- [ ] **Step 1: Add failing pure-policy tests**

Extract and test:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailRole { Invocation, Result { failed: bool } }

fn tool_detail_default_open(
    call: &ToolCall,
    resolved: bool,
    active_group: bool,
    is_last: bool,
) -> bool
```

Required cases:

```rust
assert!(tool_detail_default_open(&exec, false, true, true));
assert!(tool_detail_default_open(&exec, true, true, true));
assert!(!tool_detail_default_open(&exec, true, false, true));
assert!(!tool_detail_default_open(&read, false, true, true));
```

Add a test that an unresolved ordinary `ToolItem` selects a pending trail and a resolved Exec selects `Success` or `Failed`.

- [ ] **Step 2: Run transcript tests and verify RED**

```bash
cargo test -p zeron-ui transcript::tests --lib --no-default-features
```

Expected: compile failure for the missing policies.

- [ ] **Step 3: Implement lifecycle header rendering**

Update `chip_header_row` to use `tool_presentation`. Add a trailing status enum:

```rust
enum ChipTrail {
    Pending { key: SharedString },
    Outcome { failed: bool },
    Chevron { open: bool },
    OpenArrow,
}
```

Pending paints `mini_mono_spinner` in the fixed 18 px slot. Exec outcome paints check/X plus `Success`/`Failed` in a fixed-width compact group. Failure uses `theme.danger`; success stays `theme.text_muted` like the reference.

- [ ] **Step 4: Implement derived detail defaults**

When computing `detail_opens`, replace `fold.open.unwrap_or(false)` with:

```rust
let default_open = tool_detail_default_open(
    &tools[ix].call,
    tools[ix].resolved,
    auto_open,
    ix + 1 == tools.len(),
);
fold.open.unwrap_or(default_open)
```

Use that same default when toggling so the first click always reverses the currently painted state. Preserve group/detail height tweens and manual override behavior.

- [ ] **Step 5: Implement invocation/result tones**

Pass `DetailRole::Invocation` for `invocation` and `DetailRole::Result { failed: tool.is_error }` for `detail`. In `detail_body`:

- invocation text uses `theme.text.opacity(0.95)`;
- successful output uses `theme.text_muted`;
- failed output uses `theme.danger_muted`;
- Exec invocation prefixes the first visual line with an amber `$` using the existing syntax amber color;
- diff/stats remain unchanged.

Keep output truncation and analytic heights unchanged.

- [ ] **Step 6: Run focused tests and formatting**

```bash
cargo fmt --all -- --check
cargo test -p zeron-ui transcript::tests --lib --no-default-features
```

Expected: tests pass and group auto-open tests remain green.

- [ ] **Step 7: Commit**

```bash
git add crates/ui/src/transcript.rs
git commit -m "feat(ui): show live command tool states"
```

### Task 3: Additive execution metadata through protocol and document

**Files:**
- Modify: `crates/proto/src/agent.rs`
- Modify: `crates/doc/src/parts.rs`
- Modify: `crates/doc/src/schema.rs`
- Modify: `crates/doc/src/rebuild.rs`
- Modify: `crates/engine/src/sessions.rs`
- Test: the same modules' existing test sections

**Interfaces:**
- Produces: `ToolExecutionMeta { exit_code, duration_ms }`, `AgentEvent::ToolResult.execution`, and `MessagePart::Tool.execution`.
- Compatibility: serde/Loro absence reads as `None`; existing `output`, diff, refs and stats retain their meanings.

- [ ] **Step 1: Write failing proto serde tests**

Require both legacy absence and populated camelCase output:

```rust
let event = AgentEvent::ToolResult {
    id: "c1".into(),
    is_error: false,
    output: Some("ok".into()),
    diff: None,
    execution: Some(ToolExecutionMeta {
        exit_code: Some(0),
        duration_ms: Some(1250),
    }),
};
let value = serde_json::to_value(&event).unwrap();
assert_eq!(value["execution"]["exitCode"], 0);
assert_eq!(value["execution"]["durationMs"], 1250);
```

Also deserialize a pre-feature ToolResult JSON without `execution` and assert `None`.

- [ ] **Step 2: Run proto tests and verify RED**

```bash
cargo test -p zeron-proto --lib agent_event_round_trips
```

- [ ] **Step 3: Implement protocol metadata**

Define the serde-camelCase struct and add:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
execution: Option<ToolExecutionMeta>,
```

Update every constructor to pass `None` until Task 4 enriches supported adapters.

- [ ] **Step 4: Write failing doc fold/schema tests**

Test that execution metadata:

- lands on the matching part when ToolResult resolves;
- round-trips through `SessionDoc`;
- is absent for legacy rows;
- survives `render_parts` and rebuild;
- does not copy full output outside the existing summary policy.

- [ ] **Step 5: Implement doc/schema propagation**

Add optional `execution` to `MessagePart::Tool` and `DocPartJson`, write it under the additive `execution` key, salvage it best-effort, and include it in render/rebuild projections. Update `byte_len` and transcript fingerprints.

- [ ] **Step 6: Run focused crates**

```bash
cargo test -p zeron-proto --lib
cargo test -p zeron-doc --lib
cargo test -p zeron-engine sessions --lib
```

- [ ] **Step 7: Commit**

```bash
git add crates/proto/src/agent.rs crates/doc/src/parts.rs crates/doc/src/schema.rs crates/doc/src/rebuild.rs crates/engine/src/sessions.rs
git commit -m "feat: preserve command execution metadata"
```

### Task 4: Preserve runtime-reported command output and metadata

**Files:**
- Modify: `crates/harness/src/codex/normalize.rs`
- Modify: `crates/harness/src/claude/normalize.rs`
- Modify: `crates/harness/src/omp/normalize.rs`
- Modify: `crates/harness/src/acp/normalize.rs`
- Modify: `crates/harness/src/cursor/mod.rs`
- Test: `crates/harness/tests/{codex,claude,omp_rpc,acp,cursor}.rs` and normalizer unit tests

**Interfaces:**
- Consumes: runtime-native completed tool payloads.
- Produces: honest `ToolResult.output` and optional `ToolExecutionMeta`; never infers missing fields.

- [ ] **Step 1: Write failing adapter tests**

Add fixtures/assertions:

- Codex completed command with `aggregatedOutput: "11 alpha.txt\n"`, `exitCode: 0`, `durationMs: 4868` produces output plus metadata.
- Claude `tool_result.content` text produces bounded output but no execution metadata.
- OMP result `details.exitCode` and optional duration produce metadata while existing content remains output.
- ACP raw output plus execution fields preserve both.
- Cursor continues to emit `execution: None`.

- [ ] **Step 2: Run harness tests and verify RED**

```bash
cargo test -p zeron-harness --test codex --test claude --test omp_rpc --test acp --test cursor
```

- [ ] **Step 3: Implement Codex and Claude extraction**

Codex completed `commandExecution` reads:

```rust
output: field(item, &["aggregatedOutput", "aggregated_output"])
    .and_then(Value::as_str)
    .filter(|text| !text.is_empty())
    .map(str::to_owned),
execution: Some(ToolExecutionMeta {
    exit_code: i32::try_from(exit_code).ok(),
    duration_ms: field(item, &["durationMs", "duration_ms"]).and_then(Value::as_u64),
}),
```

Claude extracts only text-like tool-result content through a bounded helper. Exclude synthetic internal metadata and retain existing subagent behavior.

- [ ] **Step 4: Implement OMP/ACP optional metadata and fallbacks**

Read only concrete numeric fields. Return `None` when both fields are absent. Preserve existing error/output/diff behavior byte-for-byte otherwise.

- [ ] **Step 5: Run harness suites**

```bash
cargo test -p zeron-harness --test codex --test claude --test omp_rpc --test acp --test cursor
cargo test -p zeron-harness --lib
```

- [ ] **Step 6: Commit**

```bash
git add crates/harness/src crates/harness/tests
git commit -m "feat(harness): retain command result details"
```

### Task 5: Persist reasoning lifecycle in the document

**Files:**
- Modify: `crates/doc/src/parts.rs`
- Modify: `crates/doc/src/schema.rs`
- Modify: `crates/doc/src/rebuild.rs`
- Modify: `crates/engine/src/sessions.rs`
- Test: the same modules' tests

**Interfaces:**
- Produces: `MessagePart::Reasoning { id, text, completed, duration_ms }`.
- Rule: consecutive `ReasoningDelta` appends; the next visible non-reasoning event or `Done` completes the open reasoning part.

- [ ] **Step 1: Write failing fold tests**

```rust
#[test]
fn reasoning_deltas_append_and_close_before_the_next_visible_part() {
    let mut parts = Vec::new();
    fold_event_into_parts(&mut parts, &AgentEvent::ReasoningDelta { text: "one".into() });
    fold_event_into_parts(&mut parts, &AgentEvent::ReasoningDelta { text: " two".into() });
    fold_event_into_parts(&mut parts, &AgentEvent::ToolCall {
        id: "c1".into(),
        call: ToolCall::Exec { command: "pwd".into() },
    });
    assert!(matches!(&parts[0], MessagePart::Reasoning { text, completed: true, .. } if text == "one two"));
}
```

Add tests for empty heartbeat ignored, `Done` closure, schema round-trip, legacy rows, continuation join, and rebuild.

- [ ] **Step 2: Run doc tests and verify RED**

```bash
cargo test -p zeron-doc --lib reasoning
```

- [ ] **Step 3: Implement Reasoning part and closure helper**

Add:

```rust
Reasoning {
    id: String,
    text: String,
    #[serde(default)]
    completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
}
```

Use deterministic ids `r{out.len()}`. Before folding `TextDelta`, `ToolCall`, `InputRequested`, `Error`, `Steered`, or `Done`, mark a trailing open Reasoning part complete. Empty deltas remain heartbeats and create no part.

- [ ] **Step 4: Implement additive Loro schema and engine projection**

Use `kind: "reasoning"`, LoroText for the body, `completed`, and optional `durationMs`. Include it in `render_parts`, byte accounting, continuation behavior, salvage, and transcript JSON. Do not include reasoning in `folded_text` or title generation.

- [ ] **Step 5: Run doc and engine tests**

```bash
cargo test -p zeron-doc --lib
cargo test -p zeron-engine sessions --lib
```

- [ ] **Step 6: Commit**

```bash
git add crates/doc/src crates/engine/src/sessions.rs
git commit -m "feat(doc): retain reasoning transcript parts"
```

### Task 6: GPUI Thinking/Thought component

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `MessagePart::Reasoning`.
- Produces: `RowKind::Reasoning`, stable row fingerprint, active elapsed state, fold state, and Markdown trace rendering.

- [ ] **Step 1: Write failing row lifecycle tests**

Require:

- active reasoning produces one `RowKind::Reasoning { active: true }` row;
- settled reasoning keeps the same row id and flips only its version/state;
- reasoning stays separate from final Markdown and ToolGroup rows;
- empty reasoning creates no row;
- active reasoning defaults open, settled defaults closed;
- measured duration formats as `Thought for 4 seconds`.

- [ ] **Step 2: Run transcript tests and verify RED**

```bash
cargo test -p zeron-ui transcript::tests --lib --no-default-features
```

- [ ] **Step 3: Implement row model and stable timing**

Add a reasoning row with `text`, `tree`, `active`, and optional measured duration. Keep a render-local `reasoning_started: HashMap<SharedString, Instant>` keyed by row id for live elapsed display; remove it when the row settles or is evicted. A remounted historical active row may restart the visual elapsed clock, but settled rows never invent duration.

- [ ] **Step 4: Implement the component**

Render:

- 14 px sparkle in muted tint;
- one-pass shimmer/paint-local loader for active `Thinking`;
- monospaced elapsed seconds updated at 250 ms only while visible and active;
- muted `Thought` or formatted measured duration when settled;
- disclosure chevron;
- a one-pixel trace rail and existing Markdown renderer at compact size;
- active default open, settled default closed, manual override in the existing fold map.

Use analytic height from the reasoning tree's existing block metrics; do not introduce DOM-style measurement or a second Markdown parser.

- [ ] **Step 5: Run focused tests and formatting**

```bash
cargo fmt --all -- --check
cargo test -p zeron-ui transcript::tests --lib --no-default-features
```

- [ ] **Step 6: Commit**

```bash
git add crates/ui/src/transcript.rs
git commit -m "feat(ui): render live reasoning traces"
```

### Task 7: Integration gates and bounded visual validation

**Files:**
- Modify only if a gate exposes a feature regression.
- Verify all files changed by Tasks 1-6.

**Interfaces:**
- Produces: a clean, reviewable branch and real-app parity evidence.

- [ ] **Step 1: Run focused functional suites**

```bash
cargo test -p zeron-proto --lib
cargo test -p zeron-doc --lib
cargo test -p zeron-harness --lib
cargo test -p zeron-harness --test codex --test claude --test omp_rpc --test acp --test cursor
cargo test -p zeron-engine sessions --lib
cargo test -p zeron-ui transcript::tests --lib --no-default-features
```

- [ ] **Step 2: Run workspace gates**

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo build -p zeron
git diff --check
```

- [ ] **Step 3: Launch the dev app and exercise three runtimes**

```bash
RUST_LOG=warn cargo run -p zeron
```

Exercise one command with output and one reasoning-capable turn in Claude, Codex, and OMP. Confirm spinner, verbs, success/failure, active disclosure, settled collapse, output tones, Thought lifecycle, no layout movement, and old-session rendering.

- [ ] **Step 4: Perform one bounded visual comparison**

Compare against the Orchestrator.dev reference screenshots and the design spec. Record all defects in one pass, apply one cohesive correction batch, and run one confirmation pass. Do not enter an open-ended polish loop.

- [ ] **Step 5: Run the final review gate**

Inspect `git diff --stat`, `git diff --check`, branch status, and every commit. Confirm no unrelated changes, no push, and no mutation of Pi or runtime selection behavior.

- [ ] **Step 6: Commit only if validation required a correction**

```bash
git add <validated-files>
git commit -m "fix(ui): polish streaming tool parity"
```
