# Assistant Turn Steps Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every eligible Comet assistant message one Orchestrator-style operational disclosure that folds completed work while leaving current activity and the final answer expanded.

**Architecture:** Add a pure `turn_steps` policy over durable `MessagePart` values, then project the completed prefix into a composite `RowKind::TurnSteps` without changing runtime protocols or the composer. Reuse all existing child renderers and stable row ids by separating transcript row-body rendering from top-level list chrome.

**Tech Stack:** Rust 2024, GPUI virtualized list, `zeron-doc` message parts, existing transcript/render cache/fold infrastructure, Cargo tests.

**Spec:** `docs/plans/2026-08-23-assistant-turn-steps-design.md`

## Global Constraints

- Preserve the current native composer without layout, styling, or behavior changes.
- Do not change runtime protocols, harness adapters, or durable document schemas.
- A running subagent, unresolved tool, active reasoning part, or unresolved input must never be folded.
- Settled turns collapse only when a non-empty final text follows the last tool.
- Keep specialized child renderers, child fold choices, inline images, Mermaid, todo cards, and tool sidecar affordances.
- Preserve transcript virtualization, minimal row splices, stable ids, bottom pinning, own-turn runway, hover timestamps, and render caches.
- Use TDD and commit each task locally. Do not push, release, or promote branches.

---

## File Structure

- Create `crates/ui/src/turn_steps.rs`: pure split policy, activity classification, summary formatting, and unit tests.
- Modify `crates/ui/src/lib.rs`: register the private `turn_steps` module.
- Modify `crates/ui/src/transcript.rs`: source-indexed row projection, composite row model/version, body renderer extraction, disclosure rendering/state, recursive cache invalidation, and transcript tests.
- Modify `docs/PARITY.md`: record assistant-turn disclosure parity after the live gate passes.

### Task 1: Pure assistant-turn boundary and summary policy

**Files:**
- Create: `crates/ui/src/turn_steps.rs`
- Modify: `crates/ui/src/lib.rs`
- Test: `crates/ui/src/turn_steps.rs`

**Interfaces:**
- Consumes: `&[zeron_doc::MessagePart]` and `Option<zeron_doc::MessageStatus>`.
- Produces: `TurnStepsMode`, `TurnStepsPlan`, and `plan_turn_steps(parts, status) -> Option<TurnStepsPlan>`.

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

Build a prefix containing one agent, two reads (`ReadFile` + `Search`), one web
search, two edits, one command, one todo, and one generic MCP call. Require:

```rust
assert_eq!(activity_breakdown(&parts), "1 agent, 2 reads, 1 search, 2 edits, 1 command, 2 tools");
```

Also assert correct singular/plural forms and `None`/generic step fallback when
the prefix has no categorizable tool.

- [ ] **Step 8: Implement classification and run the module tests GREEN**

Implement fixed-order buckets exactly as specified in the design, then run:

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

### Task 2: Project a stable composite TurnSteps row

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `turn_steps::plan_turn_steps`, existing `rows_for_entry_with_todo_history`, and existing `Row`/`RowKind` values.
- Produces: `RowKind::TurnSteps { rows, summary }`, private `ProjectedRow`, and `turn_steps_version`.

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
    rows: &[Row],
) -> u64;
```

Hash a delimiter-safe sequence of mode, summary length/bytes, child id
length/bytes, and child version. Use id `{entry.id}#steps`, `turn_start` from the
first folded child, and no timestamp. Leave the settled entry timestamp on the
last top-level final-answer row.

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

### Task 3: Render and persist the per-message disclosure

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Test: `crates/ui/src/transcript.rs`

**Interfaces:**
- Consumes: `RowKind::TurnSteps`, existing child row renderers, and stable row ids.
- Produces: `render_row_body`, `render_turn_steps`, `turn_steps_is_open`,
  `toggle_turn_steps_state`, and `Transcript::turn_steps_open`.

- [ ] **Step 1: Write failing disclosure-state tests**

Add pure state helpers and tests requiring independent stable keys:

```rust
#[test]
fn turn_step_disclosures_are_closed_by_default_and_independent() {
    let mut state = HashMap::new();
    assert!(!turn_steps_is_open(&state, "a#steps"));
    toggle_turn_steps_state(&mut state, "a#steps".into());
    assert!(turn_steps_is_open(&state, "a#steps"));
    assert!(!turn_steps_is_open(&state, "b#steps"));
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
```

The default is false. The click listener calls `toggle_turn_steps_state` and
then `cx.notify()`.

- [ ] **Step 5: Render the TurnSteps header and children**

Implement `render_turn_steps` with:

- a 26px clickable header;
- `crate::icons::CHECKLIST` at 14px as the steps icon;
- 12px muted summary text;
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

### Task 4: Protect caches, virtualization, and streaming transitions

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

### Task 5: Full validation and parity documentation

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
least two tool categories, and a final text answer.

- [ ] **Step 4: Verify the real UI behavior**

For each runtime confirm:

- completed work folds under one categorized header;
- the current activity stays visible while streaming;
- concurrent unresolved calls remain visible;
- the final answer stays expanded after completion;
- expanding shows original Thought/tool/todo/media components in order;
- inner tool detail folds remain independently controllable;
- stopping or aborting without a final answer does not invent a turn summary;
- scrolling, return-to-bottom, chat switching, and old transcript reopening do
  not jump or forget disclosure state within the active chat.

- [ ] **Step 5: Update the parity record**

In `docs/PARITY.md`, change the transcript line to explicitly include:

```markdown
assistant-turn operational prefix disclosure with categorized summary,
streaming current-activity preservation, and final-answer separation
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
