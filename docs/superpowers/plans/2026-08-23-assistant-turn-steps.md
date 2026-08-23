# Assistant Turn Steps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every eligible Comet assistant message one Orchestrator-style operational disclosure and specialized live Write/Edit cards with bounded green/red previews, persisted turn duration, and journal-backed full-content expansion.

**Architecture:** Persist engine-measured duration on each assistant message, preserve upstream Write/Edit input deltas as same-id ToolCall refreshes, and store only a bounded semantic file preview in the synchronized doc. Project completed work into `TurnSteps`; render active file changes through a specialized card whose full historical content is fetched lazily from the owning device's run journal.

**Tech Stack:** Rust 2024, GPUI virtualized list, `zeron-doc` message parts, existing transcript/render cache/fold infrastructure, Cargo tests.

**Spec:** `docs/plans/2026-08-23-assistant-turn-steps-design.md`

## Global Constraints

- Preserve the current native composer without layout, styling, or behavior changes.
- Do not change runtime protocols or harness adapters; the only durable schema
  change is additive `SessionMessageEntry.duration_ms: Option<u64>`.
- Turn duration is engine-measured from the assistant segment boundary; never
  derive it from mutable session rows or provider-specific tool durations.
- A running subagent, unresolved tool, active reasoning part, or unresolved input must never be folded.
- Settled turns collapse only when a non-empty final text follows the last tool.
- Keep specialized child renderers, child fold choices, inline images, Mermaid, todo cards, and tool sidecar affordances.
- Preserve the existing privacy rule that strips complete Write/Edit bodies from
  synchronized docs; full input is fetched only on explicit expansion.
- Never fabricate file content for runtimes that expose only path/kind.
- A live file preview retains at most 15 lines of 512 Unicode scalar values;
  full-input RPC responses are capped at 1 MiB.
- Preserve transcript virtualization, minimal row splices, stable ids, bottom pinning, own-turn runway, hover timestamps, and render caches.
- Use TDD and commit each task locally. Do not push, release, or promote branches.

---

## File Structure

- Modify `crates/doc/src/schema.rs`: additive assistant-entry duration,
  Loro/JSON read-write, continuation joining, segment finalization, and tests.
- Modify `crates/doc/src/transcript_delta.rs`: include duration in entry-change
  detection.
- Modify `crates/engine/src/sessions.rs`: compute duration at every parent and
  subagent segment finish.
- Modify existing `SessionMessageEntry` literals under `crates/`: initialize
  `duration_ms` explicitly in production constructors and fixtures.
- Create `crates/harness/src/partial_tool_input.rs`: focused partial JSON string
  extraction for Write/Edit fields.
- Modify `crates/harness/src/claude/{wire.rs,normalize.rs}`: preserve tool-use
  start metadata and incremental input JSON as same-id typed call refreshes.
- Modify `crates/harness/src/omp/normalize.rs`: preserve OMP
  `toolcall_start|delta|end` updates through the same typed refresh contract.
- Test `crates/harness/src/acp/normalize.rs`: prove shape-bearing rawInput
  updates refresh Write/Edit without a new adapter path.
- Modify `crates/doc/src/parts.rs` and `crates/doc/src/schema.rs`: additive
  bounded `FileChangePreview` derivation and persistence.
- Modify `crates/engine/src/run_journal.rs`, `crates/engine/src/sessions.rs`,
  `crates/engine/src/rpc.rs`, and `crates/rpc/src/lib.rs`: safe historical
  `FetchToolInput` lookup on the owning device.
- Create `crates/ui/src/file_change.rs`: preview geometry, duration-independent
  file-card formatting, and pure line/render decisions.
- Create `crates/ui/src/turn_steps.rs`: pure split policy, activity classification, summary formatting, and unit tests.
- Modify `crates/ui/src/lib.rs`: register the private `turn_steps` module.
- Modify `crates/ui/src/transcript.rs`: source-indexed row projection, composite row model/version, body renderer extraction, disclosure rendering/state, recursive cache invalidation, and transcript tests.
- Modify `docs/PARITY.md`: record assistant-turn disclosure parity after the live gate passes.

### Task 1: Persist authoritative assistant-segment duration

**Files:**
- Modify: `crates/doc/src/schema.rs`
- Modify: `crates/doc/src/transcript_delta.rs`
- Modify: `crates/engine/src/sessions.rs`
- Modify: existing `SessionMessageEntry` literals found by `rg -n "SessionMessageEntry \\{" crates --glob '*.rs'`
- Test: `crates/doc/src/schema.rs`
- Test: `crates/doc/src/transcript_delta.rs`
- Test: `crates/engine/src/sessions.rs`

**Interfaces:**
- Consumes: `segment_started: i64`, terminal segment time from `now_ms()`, and existing `SegmentWriter` lifecycle.
- Produces: `SessionMessageEntry.duration_ms: Option<u64>`, `segment_duration_ms(started_at, finished_at) -> u64`, and `SegmentWriter::finish(folded, status, duration_ms)`.

- [ ] **Step 1: Write failing schema round-trip and legacy tests**

Add `duration_ms: Option<u64>` expectations to the real entry serialization
tests in `crates/doc/src/schema.rs`:

```rust
#[test]
fn assistant_entry_duration_round_trips_and_legacy_entries_default_to_none() {
    let mut entry = user_entry("assistant-1", "done");
    entry.role = MessageRole::Assistant;
    entry.duration_ms = Some(12_500);
    let doc = SessionDoc::new("chat-duration");
    doc.push_message(&entry).unwrap();

    let read = doc.read_entries().unwrap();
    assert_eq!(read[0].duration_ms, Some(12_500));

    let legacy = entry_from_json(serde_json::json!({
        "id": "legacy",
        "role": "assistant",
        "parts": [],
        "createdAt": 1,
        "deviceId": "device",
        "status": "complete"
    })).unwrap();
    assert_eq!(legacy.duration_ms, None);
}
```

Add a salvage case containing numeric `durationMs` and require preservation.

- [ ] **Step 2: Run schema tests and verify RED**

```bash
cargo test -p zeron-doc --lib assistant_entry_duration -- --nocapture
```

Expected: compile failure because `SessionMessageEntry` has no `duration_ms`.

- [ ] **Step 3: Add the additive duration field to every schema path**

Extend the type:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub duration_ms: Option<u64>,
```

Thread `durationMs` through:

- `write_entry_scalar_fields`;
- strict `RawEntry` JSON parsing;
- Loro map reading;
- salvage parsing;
- `SegmentWriter::begin` with `duration_ms: None`;
- `SegmentWriter::finish(..., duration_ms: Option<u64>)`, inserting the scalar
  before the final commit;
- continuation joining, where a continuation's non-`None` duration fills the
  root only when the root has none.

Update every existing `SessionMessageEntry` literal under `crates/` with
`duration_ms: None`, except tests/constructors intentionally exercising a real
duration. Do not use a blind whole-file rewrite outside those literals.

- [ ] **Step 4: Add failing engine duration tests**

Add pure and integration assertions:

```rust
#[test]
fn segment_duration_clamps_clock_regressions() {
    assert_eq!(segment_duration_ms(1_000, 13_500), 12_500);
    assert_eq!(segment_duration_ms(13_500, 1_000), 0);
}
```

Extend the existing run/segment tests to require nonzero `duration_ms` on:

- completed `Done`;
- interrupted `Done`;
- errored `Done`;
- quiesced completion;
- a segment finalized by `Steered`;
- a finished subagent transcript.

Add a recovery assertion that `set_message_status` does not invent duration for
an old streaming entry.

- [ ] **Step 5: Run engine tests and verify RED**

```bash
cargo test -p zeron-engine segment_duration -- --nocapture
cargo test -p zeron-engine parked_steer -- --nocapture
```

Expected: completed entries still read `duration_ms == None`.

- [ ] **Step 6: Compute duration at the engine-owned segment boundary**

Add:

```rust
fn segment_duration_ms(started_at: i64, finished_at: i64) -> u64 {
    finished_at.saturating_sub(started_at).max(0) as u64
}
```

Inside `finish_segment`, compute once from `started_at` and `now_ms()`, then pass
`Some(duration_ms)` to both existing writer branches. In `SubagentSink::finish`,
compute from `self.started_at` immediately before its writer is finalized.
Every caller already routes through these two functions, covering normal,
interrupted, errored, quiesced, steered, parked, and subagent segments without
provider-specific changes.

- [ ] **Step 7: Include duration in transcript delta equality**

In `crates/doc/src/transcript_delta.rs`, add
`prev.duration_ms != next.duration_ms` to the entry metadata comparison. Add a
test where ids/parts/status match and only duration changes; require the entry
to appear in the delta.

- [ ] **Step 8: Run focused and full schema/engine tests GREEN**

```bash
cargo test -p zeron-doc --lib
cargo test -p zeron-engine segment_duration -- --nocapture
cargo test -p zeron-engine parked_steer -- --nocapture
cargo check --workspace
cargo fmt --all -- --check
```

Expected: all commands pass and old serialized entries still load.

- [ ] **Step 9: Commit duration persistence**

```bash
git add \
  crates/doc/examples/gen_fixture.rs \
  crates/doc/src/rebuild.rs \
  crates/doc/src/schema.rs \
  crates/doc/src/transcript_delta.rs \
  crates/engine/src/doc_host.rs \
  crates/engine/src/sessions.rs \
  crates/engine/tests/e2e.rs \
  crates/engine/tests/local_import.rs \
  crates/engine/tests/restart_resume.rs \
  crates/engine/tests/transcript_salvage.rs \
  crates/ui/src/composer.rs \
  crates/ui/src/details_sidebar/chat_workers.rs \
  crates/ui/src/details_sidebar/todos.rs \
  crates/ui/src/rail.rs \
  crates/ui/src/shell.rs \
  crates/ui/src/state.rs \
  crates/ui/src/transcript.rs
git commit -m "feat(chat): persist assistant turn duration"
```

Before committing, inspect `git diff --cached --name-only` and remove any path
not required by the `SessionMessageEntry.duration_ms` compile migration.

### Task 2: Pure assistant-turn boundary and summary policy

**Files:**
- Create: `crates/ui/src/turn_steps.rs`
- Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/src/turn_steps.rs`

**Interfaces:**
- Consumes: `&[zeron_doc::MessagePart]` and `Option<zeron_doc::MessageStatus>`.
- Produces: `TurnStepsMode`, `TurnStepsPlan`, `ActivityBucket`,
  `activity_breakdown(parts)`, and
  `plan_turn_steps(parts, status) -> Option<TurnStepsPlan>`.

- [ ] **Step 1: Write failing settled-boundary tests**

Create `crates/ui/src/turn_steps.rs`, register `mod turn_steps;` in
`crates/ui/src/lib.rs`, and add tests for this contract:

```rust
#[test]
fn settled_turn_folds_everything_before_text_after_the_last_tool() {
    let parts = vec![
        text("narration", "Inspecting"),
        tool("read", ToolCall::ReadFile { path: "src/lib.rs".into() }, true),
        text("answer", "The issue is fixed."),
    ];

    let plan = plan_turn_steps(&parts, Some(MessageStatus::Complete)).unwrap();
    assert_eq!(plan.mode, TurnStepsMode::FinalAnswer);
    assert_eq!(plan.split_before_part, 2);
    assert_eq!(plan.summary, "1 read");
}

#[test]
fn settled_turn_without_text_after_its_last_tool_stays_unwrapped() {
    let parts = vec![
        text("narration", "Inspecting"),
        tool("read", ToolCall::ReadFile { path: "src/lib.rs".into() }, true),
    ];
    assert_eq!(plan_turn_steps(&parts, Some(MessageStatus::Complete)), None);
}
```

Test helpers must construct real `MessagePart` values and set nonessential tool
fields to `None`/empty values. Do not introduce a test-only production type.

- [ ] **Step 2: Run the settled tests and verify RED**

Run:

```bash
cargo test -p zeron-ui --lib settled_turn -- --nocapture
```

Expected: compile failure because `TurnStepsPlan` and `plan_turn_steps` do not
exist.

- [ ] **Step 3: Implement the settled policy minimally**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStepsMode {
    StreamingPrefix,
    FinalAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnStepsPlan {
    pub split_before_part: usize,
    pub mode: TurnStepsMode,
    pub summary: String,
}

pub fn plan_turn_steps(
    parts: &[MessagePart],
    status: Option<MessageStatus>,
) -> Option<TurnStepsPlan>
```

For a non-streaming status, locate the last `MessagePart::Tool` and last
non-empty `MessagePart::Text`. Return `None` unless the text index is greater
than the tool index. Set `split_before_part` to the text index.

- [ ] **Step 4: Add failing streaming-boundary tests**

Add cases proving:

```rust
#[test]
fn streaming_turn_keeps_all_concurrent_unresolved_tools_visible() {
    let parts = vec![
        tool("old", exec("cargo check"), true),
        tool("first", exec("cargo test -p zeron-ui"), false),
        tool("second", ToolCall::ReadFile { path: "Cargo.toml".into() }, false),
    ];
    let plan = plan_turn_steps(&parts, Some(MessageStatus::Streaming)).unwrap();
    assert_eq!(plan.mode, TurnStepsMode::StreamingPrefix);
    assert_eq!(plan.split_before_part, 1);
}

#[test]
fn streaming_turn_without_unresolved_work_keeps_latest_activity_visible() {
    let parts = vec![
        tool("read", ToolCall::ReadFile { path: "src/lib.rs".into() }, true),
        text("latest", "Preparing the result"),
    ];
    assert_eq!(
        plan_turn_steps(&parts, Some(MessageStatus::Streaming))
            .unwrap()
            .split_before_part,
        1,
    );
}
```

Add separate cases for active `Reasoning`, unresolved `Input`, and a tool whose
`subagent_status` is `Running` despite `resolved == true`.

- [ ] **Step 5: Run the streaming tests and verify RED**

```bash
cargo test -p zeron-ui --lib streaming_turn -- --nocapture
```

Expected: the settled-only policy returns the wrong boundary or `None`.

- [ ] **Step 6: Implement unsettled activity detection**

Add private helpers:

```rust
fn is_visible_part(part: &MessagePart) -> bool;
fn is_unsettled_part(part: &MessagePart) -> bool;
fn streaming_split(parts: &[MessagePart]) -> Option<usize>;
```

`is_unsettled_part` returns true for unresolved tools, running subagents, active
reasoning, and unresolved input. `WorkflowTask` and whitespace text are not
visible. Choose the first unsettled index; if none exists, choose the last
visible index. Reject index zero and prefixes without visible operational work.

- [ ] **Step 7: Write failing exact-summary tests**

Build a prefix containing one agent, one skill, two reads (`ReadFile` +
`Search`), one web search, two edits, one command, two waits (one OMP
`Unknown { name: "hub", input: {"op":"wait"} }` plus one MCP
`wait_for_agent`), one message send, one terminal listing, one terminal capture,
one todo, and one generic MCP call. Require:

```rust
assert_eq!(
    activity_breakdown(&parts),
    "1 agent, 1 skill, 2 reads, 1 search, 2 edits, 1 command, 2 waits, 1 message, 1 terminal, 1 capture, 2 tools",
);
```

Add a separate test that mixes a built-in agent call and an MCP
`create_agent`; require `2 agents` once, never two `agent` segments. Also assert
correct singular/plural forms and empty-summary fallback when the prefix has no
categorizable tool.

- [ ] **Step 8: Implement classification and run the module tests GREEN**

Implement:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActivityBucket {
    Agent,
    Skill,
    Read,
    Search,
    Edit,
    Command,
    Wait,
    Message,
    Terminal,
    Capture,
    Tool,
}

fn activity_bucket(call: &ToolCall) -> ActivityBucket;
pub fn activity_breakdown(parts: &[MessagePart]) -> String;
```

Use one count map keyed by `ActivityBucket`; do not maintain separate built-in
and MCP maps. Classify OMP `hub` by `input.op` and exact MCP names from the
design. Emit buckets in the design's fixed order, then run:

```bash
cargo test -p zeron-ui --lib turn_steps -- --nocapture
cargo fmt -p zeron-ui -- --check
```

Expected: all `turn_steps` tests pass and formatting is clean.

- [ ] **Step 9: Commit the pure policy**

```bash
git add crates/ui/src/lib.rs crates/ui/src/turn_steps.rs
git commit -m "feat(ui): define assistant turn collapse policy"
```

### Task 3: Project a stable composite TurnSteps row

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `turn_steps::plan_turn_steps`, existing `rows_for_entry_with_todo_history`, and existing `Row`/`RowKind` values.
- Produces: `RowKind::TurnSteps { rows, summary, duration_ms }`, private
  `ProjectedRow`, and `turn_steps_version`.

- [ ] **Step 1: Write a failing final-answer projection test**

Add a complete assistant entry containing text, Read, intervening text, Exec,
and final text. Require the top-level projection to be exactly:

```rust
assert!(matches!(rows[0].kind, RowKind::TurnSteps { .. }));
assert_eq!(rows[0].id.as_ref(), "assistant-1#steps");
assert!(matches!(rows[1].kind, RowKind::Markdown { .. }));
```

Inspect the composite children and assert that both tool groups and the
intervening Markdown rows are retained in source order.

- [ ] **Step 2: Run the projection test and verify RED**

```bash
cargo test -p zeron-ui --lib assistant_turn_projects -- --nocapture
```

Expected: the current builder emits multiple top-level Markdown/ToolGroup rows.

- [ ] **Step 3: Add the composite row model and source-indexed drafts**

Extend `RowKind`:

```rust
TurnSteps {
    rows: Arc<Vec<Row>>,
    summary: SharedString,
    duration_ms: Option<u64>,
},
```

Add the private builder type:

```rust
struct ProjectedRow {
    source_start: usize,
    source_end: usize,
    row: Row,
}
```

Change the internal builder to accumulate `ProjectedRow`. Markdown blocks,
reasoning, todo/input/error, and inline-image rows use the owning part index for
both endpoints. Tool groups record the first and last tool part indices.

- [ ] **Step 4: Flush groups at the semantic split**

Compute `TurnStepsPlan` before walking parts. In the tool arm, flush
`pending_group` before consuming `part_ix == split_before_part`. This guarantees
that a resolved prefix group never contains the first unresolved/current tool.

After projection, partition rows by `source_end < split_before_part`. Wrap the
prefix only when it is non-empty. Keep the remaining rows top-level.

- [ ] **Step 5: Normalize folded-prefix child defaults**

Before storing children:

```rust
fn settle_turn_steps_child(row: &mut Row) {
    match &mut row.kind {
        RowKind::LiveMarkdown { tree, block_ix } => {
            row.kind = RowKind::Markdown {
                tree: tree.clone(),
                block_ix: *block_ix,
            };
        }
        RowKind::ToolGroup { auto_open, detail_auto_open, .. } => {
            *auto_open = false;
            *detail_auto_open = false;
        }
        _ => {}
    }
}
```

Implement this without assigning to `row.kind` while it is mutably borrowed;
use a cloned replacement value or a helper returning `Option<RowKind>`.

- [ ] **Step 6: Add stable versioning and timestamp ownership**

Add:

```rust
fn turn_steps_version(
    mode: TurnStepsMode,
    summary: &str,
    duration_ms: Option<u64>,
    rows: &[Row],
) -> u64;
```

Hash a delimiter-safe sequence of mode, summary length/bytes, `duration_ms`,
child id length/bytes, and child version. Use id
`{entry.id}#steps`, copy `entry.duration_ms` onto the composite, take
`turn_start` from the first folded child, and use no timestamp. Leave the
settled entry timestamp on the last top-level final-answer row.

- [ ] **Step 7: Cover streaming movement and edge cases**

Add tests proving:

- completed prefix enters `TurnSteps` while two unresolved tools stay top-level;
- the latest streaming text stays top-level when no tool is unresolved;
- active reasoning, unresolved input, and running subagents stay top-level;
- completed/failed subagents may enter the composite;
- a settled turn without final text retains the old independent rows;
- a todo snapshot and inline image before the boundary remain children;
- live-to-complete keeps `assistant-id#steps` stable.

- [ ] **Step 8: Run focused and full transcript model tests**

```bash
cargo test -p zeron-ui --lib assistant_turn -- --nocapture
cargo test -p zeron-ui --lib transcript -- --nocapture
```

Expected: all projection and existing transcript tests pass.

- [ ] **Step 9: Commit the row projection**

```bash
git add crates/ui/src/transcript.rs
git commit -m "feat(ui): project assistant work into turn steps"
```

### Task 4: Render and persist the per-message disclosure

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `RowKind::TurnSteps`, existing child row renderers, and stable row ids.
- Produces: `render_row_body`, `render_turn_steps`, `turn_steps_is_open`,
  `toggle_turn_steps_state`, `format_turn_duration`, and
  `Transcript::turn_steps_open`.

- [ ] **Step 1: Write failing disclosure-state tests**

Add pure state and duration helpers with tests requiring independent stable
keys and Orchestrator-compatible formatting:

```rust
#[test]
fn turn_step_disclosures_are_closed_by_default_and_independent() {
    let mut state = HashMap::new();
    assert!(!turn_steps_is_open(&state, "a#steps"));
    toggle_turn_steps_state(&mut state, "a#steps".into());
    assert!(turn_steps_is_open(&state, "a#steps"));
    assert!(!turn_steps_is_open(&state, "b#steps"));
}

#[test]
fn turn_duration_matches_the_reference_thresholds() {
    assert_eq!(format_turn_duration(850), "850ms");
    assert_eq!(format_turn_duration(12_500), "12.5s");
    assert_eq!(format_turn_duration(125_000), "2m 5s");
}
```

- [ ] **Step 2: Run the state test and verify RED**

```bash
cargo test -p zeron-ui --lib turn_step_disclosures -- --nocapture
```

Expected: compile failure because the helpers/state do not exist.

- [ ] **Step 3: Extract row-body rendering**

Move the existing `match &row.kind` from `render_row` into:

```rust
fn render_row_body(
    &mut self,
    row: &Row,
    window: &mut Window,
    theme: &Theme,
    cx: &mut Context<Self>,
) -> AnyElement;
```

Keep top gap, bottom pad, gutters, timestamp strip, trailer, hover ownership,
and max-width wrapper in `render_row`. The extraction must be behavior-neutral
for every existing row kind.

- [ ] **Step 4: Add disclosure state to Transcript**

Add:

```rust
turn_steps_open: HashMap<SharedString, bool>,
```

Initialize it empty, clear it on chat attachment, and add helpers:

```rust
fn turn_steps_is_open(state: &HashMap<SharedString, bool>, id: &str) -> bool;
fn toggle_turn_steps_state(
    state: &mut HashMap<SharedString, bool>,
    id: SharedString,
);

fn format_turn_duration(duration_ms: u64) -> String;
```

The default is false. The click listener calls `toggle_turn_steps_state` and
then `cx.notify()`.

- [ ] **Step 5: Render the TurnSteps header and children**

Implement `render_turn_steps` with:

- a 26px clickable header;
- `crate::icons::CHECKLIST` at 14px as the steps icon;
- 12px muted summary text;
- when `duration_ms > 0`, a right-aligned 10px monospaced duration immediately
  before the chevron, colored `theme.text_muted.opacity(0.5)`;
- the same 18px muted chevron tile used by `render_tool_group`, showing `▸`
  closed and `▾` open;
- a stable `{row_id}-hdr` element id and pointer interaction matching the
  existing native ToolGroup header;
- no child construction while closed;
- an expanded `flex_col` child container with 6px gaps;
- each child rendered through `render_row_body`, never `render_row`.

Do not add an outer height tween. Existing nested ToolGroup/detail animations
remain unchanged.

- [ ] **Step 6: Add render-structure tests**

Use the existing GPUI test harness patterns in `transcript.rs` to assert:

- a closed disclosure renders the header but not a known child id/content;
- toggling reveals the child content;
- toggling one assistant entry does not open another;
- a nested ToolGroup keeps its own fold state after outer close/reopen;
- `duration_ms: Some(12_500)` renders `12.5s`, while `None` and `Some(0)` render
  no duration label;
- the final-answer row remains mounted regardless of disclosure state.

- [ ] **Step 7: Run UI tests and formatting**

```bash
cargo test -p zeron-ui --lib turn_steps -- --nocapture
cargo test -p zeron-ui --lib transcript -- --nocapture
cargo fmt -p zeron-ui -- --check
```

Expected: disclosure tests and all existing transcript tests pass.

- [ ] **Step 8: Commit the disclosure renderer**

```bash
git add crates/ui/src/transcript.rs
git commit -m "feat(ui): render per-turn assistant steps"
```

### Task 5: Protect caches, virtualization, and streaming transitions

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: composite row trees, `RenderCache`, `diff_rows`, row cache, veil state, list remeasurement, and `turn_steps_open`.
- Produces: recursive invalidation/pruning with stable live-to-settled behavior.

- [ ] **Step 1: Write a failing recursive-invalidation test**

Create a composite row with a Markdown child and assert the helper returns both
ids in deterministic order:

```rust
assert_eq!(
    row_render_ids(&steps_row),
    vec![SharedString::from("assistant#steps"), SharedString::from("assistant#text.0")],
);
```

Add a replacement case whose child version changes while the composite id stays
stable and require the child cache id to be invalidated.

- [ ] **Step 2: Run the invalidation test and verify RED**

```bash
cargo test -p zeron-ui --lib turn_steps_invalidation -- --nocapture
```

Expected: the current sync invalidates only the top-level row id.

- [ ] **Step 3: Implement recursive row-tree invalidation**

Add:

```rust
fn visit_row_ids(row: &Row, visit: &mut impl FnMut(&SharedString));
```

Visit the row id first, then recurse into `TurnSteps.rows`. Replace the direct
`render_cache.invalidate_row(&row.id)` call in `sync` with this traversal.

- [ ] **Step 4: Prune disclosure state after row sync**

Collect current `TurnSteps` ids from `new_rows`, then retain only matching keys:

```rust
self.turn_steps_open.retain(|id, _| live_turn_step_ids.contains(id));
```

Do this after `new_rows` is complete and before returning from either sync path,
including the `diff_rows == None` path.

- [ ] **Step 5: Add transition and scroll-safety regressions**

Add tests proving:

- a streaming prefix growing by one completed activity updates the composite
  version without changing its id;
- a terminal `duration_ms` arrival updates the composite version without
  changing its id or rebuilding unrelated rows;
- streaming-to-complete moves the final text outside the same composite id;
- a child moving into the disclosure does not retain a live veil;
- no timestamp is duplicated inside the composite;
- two consecutive user/assistant turns produce two independent `#steps` rows;
- old non-agentic messages produce the same row ids and kinds as before.

- [ ] **Step 6: Run the complete UI regression suite**

```bash
cargo test -p zeron-ui --lib
cargo fmt --all -- --check
git diff --check
```

Expected: the full `zeron-ui` suite passes with no formatting or whitespace
errors.

- [ ] **Step 7: Commit cache and virtualization safety**

```bash
git add crates/ui/src/transcript.rs
git commit -m "fix(ui): preserve turn steps through transcript updates"
```

### Task 6: Preserve progressive Write/Edit inputs from capable runtimes

**Files:**
- Create: `crates/harness/src/partial_tool_input.rs`
- Modify: `crates/harness/src/lib.rs`
- Modify: `crates/harness/src/claude/wire.rs`
- Modify: `crates/harness/src/claude/normalize.rs`
- Modify: `crates/harness/src/omp/normalize.rs`
- Test: `crates/harness/src/partial_tool_input.rs`
- Test: `crates/harness/src/claude/normalize.rs`
- Test: `crates/harness/src/omp/normalize.rs`
- Test: `crates/harness/src/acp/normalize.rs`

**Interfaces:**
- Consumes: Claude `content_block_start`/`input_json_delta`, OMP
  `toolcall_start|delta|end`, and ACP shape-bearing `tool_call_update` frames.
- Produces: repeated `AgentEvent::ToolCall { id, call }` updates with the same
  id and progressively decoded `WriteFile`/`EditFile` fields.

- [ ] **Step 1: Write failing focused partial-field decoder tests**

Create `crates/harness/src/partial_tool_input.rs` and tests for:

```rust
#[test]
fn decodes_complete_and_unterminated_file_strings_without_general_json_repair() {
    let raw = r#"{"file_path":"src/a.rs","content":"line 1\nline 2"#;
    assert_eq!(partial_json_string_field(raw, &["file_path"]), Some("src/a.rs".into()));
    assert_eq!(partial_json_string_field(raw, &["content"]), Some("line 1\nline 2".into()));
}

#[test]
fn incomplete_escape_is_not_invented() {
    assert_eq!(
        partial_json_string_field(r#"{"content":"line\\"#, &["content"]),
        Some("line".into()),
    );
}

#[test]
fn builds_only_file_calls_from_supported_names() {
    assert!(matches!(
        partial_file_tool_call("Write", r#"{"file_path":"a.txt","content":"hi"#),
        Some(ToolCall::WriteFile { path, content: Some(content) })
            if path == "a.txt" && content == "hi"
    ));
    assert_eq!(partial_file_tool_call("Bash", r#"{"command":"echo hi"#), None);
}
```

The decoder must ignore matching text inside another JSON string, decode
`\n|\r|\t|\\|\"` and complete `\uXXXX` escapes, and stop before an incomplete
escape. It returns only the decoded prefix of the named string field.

- [ ] **Step 2: Run decoder tests and verify RED**

```bash
cargo test -p zeron-harness partial_tool_input -- --nocapture
```

Expected: compile failure because the module/functions do not exist.

- [ ] **Step 3: Implement the bounded partial decoder**

Expose crate-private APIs:

```rust
pub(crate) fn partial_json_string_field(
    raw: &str,
    aliases: &[&str],
) -> Option<String>;

pub(crate) fn partial_file_tool_call(
    tool_name: &str,
    raw: &str,
) -> Option<ToolCall>;
```

Use a byte scanner with `in_string`, `escaped`, and JSON-key/value states. Cap
the accumulated raw input at 1 MiB; after the cap, retain the already decoded
preview but stop growing the accumulator. Map `Write|write` to path/content and
`Edit|edit` to path/old/new aliases. Do not add a JSON-repair dependency.

- [ ] **Step 4: Write failing Claude progressive Write tests**

Extend `StreamEventBody` fixtures with a tool-use start at `index: 2`, id
`tool-write`, name `Write`, followed by two `input_json_delta` frames. Require
the normalizer to emit at least two same-id calls, with the final refresh:

```rust
AgentEvent::ToolCall {
    id: "tool-write".into(),
    call: ToolCall::WriteFile {
        path: "notes/new.txt".into(),
        content: Some("first\nsecond".into()),
    },
}
```

Add an Edit case with `old_string` and a growing `new_string`. Assert that the
later full assistant tool-use frame refreshes the same id with authoritative
complete input rather than appending a second tool.

- [ ] **Step 5: Extend Claude wire state and normalize progressively**

Add to the wire structs:

```rust
pub index: usize,
pub content_block: Option<ContentBlock>,
pub partial_json: String,
```

Add `streaming_tools: HashMap<usize, StreamingToolInput>` to `Normalizer`, where
`StreamingToolInput` stores id, name, and capped raw JSON. Handle
`content_block_start`, `input_json_delta`, and `content_block_stop`; emit a
same-id ToolCall only when `partial_file_tool_call` returns a shape different
from the last emitted one. Keep empty input deltas as liveness for non-file
tools and preserve the existing subagent routing.

- [ ] **Step 6: Write and implement OMP progressive Write tests**

Push `message_update` frames with `toolcall_start`, two `toolcall_delta` values,
and `toolcall_end` under one `contentIndex`. Require same-id progressive
`WriteFile` refreshes, followed by the authoritative existing
`tool_execution_start` refresh. Store OMP partial state by content index and
clear it on end/message boundary.

- [ ] **Step 7: Lock ACP and Codex capability behavior**

Add an ACP test where two `tool_call_update` frames carry growing `rawInput`;
assert two same-id typed WriteFile calls. Add a Codex test confirming a path-only
`fileChange add` remains `WriteFile { content: None }`; the normalizer must not
read the filesystem or invent content.

- [ ] **Step 8: Run all harness normalization tests GREEN**

```bash
cargo test -p zeron-harness partial_tool_input -- --nocapture
cargo test -p zeron-harness claude -- --nocapture
cargo test -p zeron-harness omp -- --nocapture
cargo test -p zeron-harness acp -- --nocapture
cargo test -p zeron-harness codex -- --nocapture
cargo fmt --all -- --check
```

Expected: capable runtimes refresh same-id file calls; Codex degrades honestly.

- [ ] **Step 9: Commit progressive normalization**

```bash
git add \
  crates/harness/src/lib.rs \
  crates/harness/src/partial_tool_input.rs \
  crates/harness/src/claude/wire.rs \
  crates/harness/src/claude/normalize.rs \
  crates/harness/src/omp/normalize.rs \
  crates/harness/src/acp/normalize.rs \
  crates/harness/src/codex/normalize.rs
git commit -m "feat(harness): preserve progressive file tool input"
```

### Task 7: Persist a bounded semantic file-change preview

**Files:**
- Modify: `crates/doc/src/parts.rs`
- Modify: `crates/doc/src/schema.rs`
- Modify: `crates/doc/src/rebuild.rs`
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/doc/src/parts.rs`
- Test: `crates/doc/src/schema.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: same-id `ToolCall` refreshes and authoritative `ToolResult.diff`.
- Produces: `FileChangeKind`, `FileChangeLineKind`, `FileChangeLine`,
  `FileChangePreview`, `MessagePart::Tool.file_preview`, and
  `RowKind::FileChange`.

- [ ] **Step 1: Write failing bounded-preview tests**

In `crates/doc/src/parts.rs`, build a 75-line `WriteFile` and require:

```rust
let preview = file_change_preview(&call, None).unwrap();
assert_eq!(preview.kind, FileChangeKind::Write);
assert_eq!(preview.total_lines, 75);
assert_eq!(preview.additions, 75);
assert_eq!(preview.deletions, 0);
assert_eq!(preview.lines.len(), 15);
assert_eq!(preview.truncated_before, 60);
assert!(preview.lines.iter().all(|line| line.kind == FileChangeLineKind::Added));
```

Add Edit tests proving added/removed/context semantics from old/new strings and
that a provided `ToolDiff` wins over speculative call input. Add a 600-character
line test requiring exactly 512 retained Unicode scalar values.

- [ ] **Step 2: Run preview tests and verify RED**

```bash
cargo test -p zeron-doc --lib file_change_preview -- --nocapture
```

Expected: the preview types/functions do not exist.

- [ ] **Step 3: Implement preview types and derivation**

Add serializable types and constants:

```rust
pub const FILE_PREVIEW_MAX_LINES: usize = 15;
pub const FILE_PREVIEW_MAX_LINE_CHARS: usize = 512;

pub fn file_change_preview(
    call: &ToolCall,
    authoritative_diff: Option<&ToolDiff>,
) -> Option<FileChangePreview>;
```

Use `similar::TextDiff` for Edit input. Count the complete available diff,
retain the last 15 display lines, and cap each retained line by Unicode scalar
count. Path-only calls return `None`.

- [ ] **Step 4: Add the preview to MessagePart and the event fold**

Extend `MessagePart::Tool`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
file_preview: Option<FileChangePreview>,
```

On every ToolCall create/refresh, derive the preview before the call is
sanitized. On ToolResult with a diff, replace it with the authoritative preview.
`render_parts` keeps `file_preview` while continuing to strip Write/Edit bodies.
Update every affected constructor/pattern explicitly with `file_preview: None`
or `..`; do not weaken exhaustive matches globally.

- [ ] **Step 5: Persist and diff the additive field**

Thread `filePreview` through `DocPartJson`, Loro map read/write, salvage,
continuation/rebuild, `byte_len`, and transcript-delta comparison. Add a legacy
entry test with no field and a round-trip test with a 15-line preview.

- [ ] **Step 6: Project specialized file rows**

Add `file_preview: Option<Arc<FileChangePreview>>` to `ToolItem` and include it
in `tool_fingerprint`. In `rows_for_entry_with_todo_history`, flush the pending
generic group before a WriteFile/EditFile carrying a preview, then emit:

```rust
RowKind::FileChange {
    tool: ToolItem,
}
```

with id `{entry.id}#{tool_id}`. Path-only Write/Edit calls may use the same row
for an honest header without a body. The row remains eligible as a child of
`TurnSteps` after resolution.

- [ ] **Step 7: Add transcript projection regressions**

Require:

- a live Write preview updates one stable FileChange row version;
- the row stays outside TurnSteps while unresolved;
- after resolution it can move inside TurnSteps without changing its id;
- path-only Codex writes have no invented lines/counts;
- generic Exec/Read grouping remains unchanged;
- old docs without `filePreview` still render through the generic fallback.

- [ ] **Step 8: Run doc and UI model tests GREEN**

```bash
cargo test -p zeron-doc --lib file_change -- --nocapture
cargo test -p zeron-doc --lib
cargo test -p zeron-ui --lib file_change -- --nocapture
cargo test -p zeron-ui --lib transcript -- --nocapture
cargo fmt --all -- --check
```

Expected: bounded previews persist without full file bodies entering the doc.

- [ ] **Step 9: Commit bounded preview persistence**

```bash
git add crates/doc/src/parts.rs crates/doc/src/schema.rs crates/doc/src/rebuild.rs crates/ui/src/transcript.rs
git commit -m "feat(chat): persist bounded file change previews"
```

### Task 8: Render the native file card and fetch full historical input

**Files:**
- Create: `crates/ui/src/file_change.rs`
- Modify: `crates/ui/src/lib.rs`
- Modify: `crates/ui/src/transcript.rs`
- Modify: `crates/proto/src/agent.rs`
- Modify: `crates/rpc/src/lib.rs`
- Modify: `crates/engine/src/run_journal.rs`
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/engine/src/rpc.rs`
- Test: `crates/ui/src/file_change.rs`
- Test: `crates/ui/src/transcript.rs`
- Test: `crates/engine/src/run_journal.rs`
- Test: `crates/engine/src/rpc.rs`

**Interfaces:**
- Consumes: `RowKind::FileChange`, bounded preview, chat/tool id, owning device,
  and the unsanitized host run journal.
- Produces: `FileToolInputSnapshot`, relay-forwardable `FetchToolInput`,
  `FileInputLoad`, stable expansion/scroll state, and the 72px/200px card.

- [ ] **Step 1: Write failing historical journal lookup tests**

Append two same-id progressive WriteFile calls and a Done to a test journal.
Require newest-first lookup to return the complete final input:

```rust
let snapshot = journal.file_tool_input("chat", "write-1", 1_048_576).unwrap().unwrap();
assert_eq!(snapshot.path, "notes/new.txt");
assert_eq!(snapshot.content.as_deref(), Some("first\nsecond"));
assert!(!snapshot.truncated);
```

Add cases proving Edit old/new fields, a newer matching `ToolResult.diff`
overriding speculative ToolCall input, missing ids, non-file tools returning
`None`, and content over 1 MiB returning a Unicode-safe truncated snapshot.

- [ ] **Step 2: Implement the sanitized journal API**

Add to `zeron-proto`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileToolInputSnapshot {
    pub path: String,
    pub content: Option<String>,
    pub old_string: Option<String>,
    pub new_string: Option<String>,
    pub truncated: bool,
}
```

Add `RunJournal::file_tool_input(chat_id, tool_call_id, max_chars)`. Scan replay
newest-first: a matching ToolResult with `diff` returns its path/old/new text;
otherwise use the newest matching WriteFile/EditFile call. Expose it through
`SessionsEngine::file_tool_input`; never return raw MCP/Unknown inputs.

- [ ] **Step 3: Add and secure the relay-forwardable RPC**

Register `methods::FETCH_TOOL_INPUT`. Params are:

```json
{"chatId":"chat-id","toolCallId":"write-1","targetDeviceId":"host-device"}
```

Add it to the forwardable unary allowlist with a 20-second deadline. On the
target engine, require the chat to exist and belong to `doc_host.device_id()`
before calling the session lookup. Return `{ "snapshot": null }` when the
journal/id is absent and the typed snapshot otherwise. Add tests for local
success, wrong-device rejection, non-file rejection, forwarding classification,
and the 1 MiB cap.

- [ ] **Step 4: Define pure native card policy and geometry**

Create `crates/ui/src/file_change.rs` with:

```rust
pub const FILE_CARD_HEADER_HEIGHT: f32 = 28.0;
pub const FILE_CARD_COLLAPSED_BODY_HEIGHT: f32 = 72.0;
pub const FILE_CARD_EXPANDED_MAX_HEIGHT: f32 = 200.0;

pub fn file_card_action(kind: FileChangeKind, resolved: bool) -> &'static str;
pub fn file_card_can_expand(resolved: bool, has_preview: bool) -> bool;
pub fn file_card_body_height(expanded: bool, content_height: f32) -> f32;
```

Tests require `Creating/Created`, `Editing/Edited`, disabled expansion while
unresolved, exactly 72px collapsed, and `min(content_height, 200)` expanded.

- [ ] **Step 5: Add Transcript-owned expansion, fetch, and scroll state**

Add:

```rust
file_change_open: HashMap<SharedString, bool>,
file_change_inputs: HashMap<SharedString, FileInputLoad>,
file_change_scrolls: HashMap<SharedString, gpui::ScrollHandle>,
```

`FileInputLoad` is `Loading(Task<()>) | Ready(Arc<FileToolInputSnapshot>) |
Failed`. Toggle is keyed by stable row id. The first completed expansion calls
`FetchToolInput` with `chatId`, `toolCallId`, and the chat's device id. Ready
state and its `ScrollHandle` survive outer TurnSteps close/reopen and virtualized
remounts. Prune all three maps when their rows disappear; clear on chat switch.

- [ ] **Step 6: Render the specialized file-change card**

Render:

- 8–9px rounded card, hairline border, faint neutral wash;
- fixed 28px header with contextual file icon and filename;
- filename shimmer plus fixed-slot spinner while unresolved;
- resolved `+N` green and `-N` red stats;
- expand/collapse glyph only when resolved and preview/content exists;
- Added rows with green translucent wash, green left rail and green text;
- Removed rows with red equivalents; Context rows neutral;
- no syntax highlighting while unresolved;
- background `HighlightStore` request after resolution, keyed by row id/content;
- 72px clipped body while closed;
- expanded body capped at 200px with `.overflow_y_scroll()` and the row's
  tracked `ScrollHandle`;
- `truncated_before > 0` marker above the bounded tail while full input loads;
- retryable `Full content unavailable` when journal fetch fails;
- `Preview limited to 1 MiB` when the returned snapshot is truncated.

Header/body/chevron toggle the inline fold only after resolution. A separate
filename click opens the existing file preview and stops propagation.

- [ ] **Step 7: Build full lines from the fetched snapshot**

For Write, render every fetched content line as Added. For Edit, use
`similar::TextDiff` over fetched old/new strings. If the fetch returns only a
path or is unavailable, retain the bounded durable preview. Never substitute
the current workspace file for historical tool input.

- [ ] **Step 8: Add UI lifecycle and composition tests**

Cover:

- live 75-line Write shows the 15-line tail, spinner, no expand action;
- resolved Write shows `+75`, expand action, and no success inference from green;
- closed body is 72px; expanded body caps at 200px and owns a scroll handle;
- first expansion fetches once; reopen uses cache and preserves scroll;
- filename action does not toggle expansion;
- failed/missing journal keeps bounded preview with retry affordance;
- path-only Codex card has no fabricated green body;
- outer TurnSteps close/reopen preserves inner file-card expansion;
- transcript row/cache pruning removes fetch/scroll state.

- [ ] **Step 9: Run focused, RPC, and full UI tests GREEN**

```bash
cargo test -p zeron-engine run_journal::tests -- --nocapture
cargo test -p zeron-engine fetch_tool_input -- --nocapture
cargo test -p zeron-ui --lib file_change -- --nocapture
cargo test -p zeron-ui --lib transcript -- --nocapture
cargo test -p zeron-ui --lib
cargo fmt --all -- --check
git diff --check
```

Expected: historical full expansion works without full file bodies in docs.

- [ ] **Step 10: Commit the specialized card and fetch path**

```bash
git add \
  crates/proto/src/agent.rs \
  crates/rpc/src/lib.rs \
  crates/engine/src/run_journal.rs \
  crates/engine/src/sessions.rs \
  crates/engine/src/rpc.rs \
  crates/ui/src/lib.rs \
  crates/ui/src/file_change.rs \
  crates/ui/src/transcript.rs
git commit -m "feat(ui): render live file change cards"
```

### Task 9: Full validation and parity documentation

**Files:**
- Modify: `docs/PARITY.md`
- Verify: `crates/ui/src/turn_steps.rs`
- Verify: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: completed assistant-turn disclosure implementation.
- Produces: build/test evidence and an updated parity record.

- [ ] **Step 1: Run workspace-level gates**

```bash
cargo test -p zeron-proto --lib
cargo test -p zeron-doc --lib
cargo test -p zeron-engine segment_duration -- --nocapture
cargo test -p zeron-engine fetch_tool_input -- --nocapture
cargo test -p zeron-harness
cargo test -p zeron-ui --lib
cargo build -p zeron
cargo fmt --all -- --check
git diff --check
```

Expected: every command exits zero.

- [ ] **Step 2: Run the UI detector once**

```bash
node /Users/guilhermevarela/.agents/skills/impeccable/scripts/detect.mjs --json \
  crates/ui/src/turn_steps.rs crates/ui/src/transcript.rs
```

Expected: `[]`. If the detector reports a mechanical UI issue, fix only the
reported rule and rerun the affected Rust tests before one final detector pass.

- [ ] **Step 3: Rebuild and launch the clean dev app**

```bash
RUST_LOG=warn target/debug/zeron
```

Exercise one agentic turn in Claude, Codex, and OMP containing narration, at
least two tool categories, a text-file Write of more than 20 lines, an Edit,
and a final text answer.

- [ ] **Step 4: Verify the real UI behavior**

For each runtime confirm:

- completed work folds under one categorized header;
- skills, waits, peer messages, terminal operations, captures, and generic
  tools enter the same canonical bucket sequence without duplicate labels;
- the current activity stays visible while streaming;
- concurrent unresolved calls remain visible;
- the final answer stays expanded after completion;
- completed/interrupted/errored turns with recorded duration show the compact
  time immediately before the chevron, and legacy turns omit it;
- capable runtimes show a green/red file preview growing during Write/Edit;
- the pending card shows spinner and does not expose expansion;
- completion shows accurate `+N/-N`, enables expansion, and highlights only
  after the live phase;
- expanded file content is capped visually at 200px and scrolls internally;
- closing/reopening the card and outer TurnSteps preserves its inner state;
- Codex path-only changes remain honest rather than inventing file contents;
- expanding shows original Thought/tool/todo/media components in order;
- inner tool detail folds remain independently controllable;
- stopping or aborting without a final answer does not invent a turn summary;
- scrolling, return-to-bottom, chat switching, and old transcript reopening do
  not jump or forget disclosure state within the active chat.

- [ ] **Step 5: Update the parity record**

In `docs/PARITY.md`, change the transcript line to explicitly include:

```markdown
assistant-turn operational prefix disclosure with categorized summary,
persisted duration, streaming current-activity preservation, and final-answer
separation; live Write/Edit cards with bounded semantic preview and
journal-backed full-content expansion
```

Record only behavior observed in the rebuilt app; do not claim runtime parity
for any runtime that was not exercised.

- [ ] **Step 6: Commit validation documentation**

```bash
git add docs/PARITY.md
git commit -m "docs(ui): record assistant turn steps parity"
```

- [ ] **Step 7: Run the final pre-commit review gate**

Review the complete commit range against
`docs/plans/2026-08-23-assistant-turn-steps-design.md`. Require no P0/P1 findings,
then report commit hashes and validation results. Do not push or merge without a
separate user request.
