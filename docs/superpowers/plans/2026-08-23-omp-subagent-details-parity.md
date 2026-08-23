# OMP Subagent Details Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render OMP subagents as compact expandable rows containing the description, stats, child state and model already emitted by OMP.

**Architecture:** Keep the existing `WorkflowTaskUpdate` transport and durable merge untouched. Correct the pure sidebar projection so descriptions and agent counts survive, then reuse the existing workflow expansion state and progress renderer for subagent rows.

**Tech Stack:** Rust, GPUI, `zeron-proto` workflow snapshots, existing `zeron-ui` tests.

**Spec:** `docs/plans/2026-08-23-omp-subagent-details-parity-design.md`

## Global Constraints

- Never synthesize metadata absent from OMP events.
- `description` is the user-visible title; `subagent_type` remains classification.
- Reuse the existing workflow progress renderer and expansion state.
- Preserve transcript-open behavior and stable task identity.
- Do not include unrelated terminal-scroll working-tree changes.

---

### Task 1: Preserve OMP presentation metadata

**Files:**
- Modify: `crates/ui/src/details_sidebar/chat_workers.rs`
- Test: `crates/ui/src/details_sidebar/chat_workers.rs`

**Interfaces:**
- Consumes: `WorkflowTaskUpdate.description`, `agent_count`, `usage`, `progress`, and `subagent_type`.
- Produces: `ChatActivityRow` with description-first title and a preformatted stats line derived only from OMP fields.

- [ ] **Step 1: Write failing projection tests**

Add a subagent snapshot with `description: "Inspect target repository"`,
`subagent_type: "task"`, `agent_count: 1`, and one-second usage. Assert:

```rust
assert_eq!(snapshot.subagents[0].title, "Inspect target repository");
assert_eq!(snapshot.subagents[0].usage.as_deref(), Some("1 agent · 1.0s"));
```

Also assert `progress` and `subagent_type` remain unchanged.

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test -p zeron-ui omp_subagent_row_uses_description_and_native_stats -- --nocapture
```

Expected: FAIL because title currently prefers `task` and usage omits agent count.

- [ ] **Step 3: Implement minimal projection correction**

Change subagent title precedence to:

```rust
task.description.clone().or_else(|| task.subagent_type.clone())
```

Extend the existing stats formatter to accept `agent_count` and emit the same
order as Orchestrator.dev: agents, tokens, tools, duration. Do not add inferred
counts.

- [ ] **Step 4: Run projection tests and verify GREEN**

```bash
cargo test -p zeron-ui omp_subagent_row_uses_description_and_native_stats -- --nocapture
```

Expected: PASS.

### Task 2: Render expandable OMP subagent details

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/src/details_sidebar/widgets.rs`
- Test: `crates/ui/src/details_sidebar/widgets.rs`

**Interfaces:**
- Consumes: `ChatActivityRow.usage`, `.progress`, `.description`, `.status`, `.id`.
- Produces: a compact subagent header with an independent disclosure action and expanded body from `render_workflow_progress`.

- [ ] **Step 1: Write failing expansion-state test**

Add a widget-state test that synchronizes one workflow id plus two subagent ids,
toggles a subagent, reorders all ids, and asserts the expansion remains bound to
the subagent id.

- [ ] **Step 2: Run the state test and verify RED**

```bash
cargo test -p zeron-ui workers_widget_keeps_subagent_expansion_bound_to_identity -- --nocapture
```

Expected: FAIL because synchronization currently receives workflow ids only.

- [ ] **Step 3: Generalize shared activity expansion state**

Rename the internal workflow-specific expansion map and methods to activity
terminology, preserving behavior:

```rust
sync_activities(ids)
toggle_activity_with_default(id, default)
activity_expanded_with_default(id, default)
```

Update workflow callers and synchronize the combined workflow + subagent id
sequence.

- [ ] **Step 4: Run the state tests and verify GREEN**

```bash
cargo test -p zeron-ui workers_widget_keeps_subagent_expansion_bound_to_identity -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Port the natural subagent row**

Change `render_subagent_row` to `&mut self`. A row is collapsible only when
`usage.is_some() || !progress.is_empty()`. Render the existing disclosure icon,
status, bot icon and description title in the header. The disclosure click only
toggles expansion; the title/body click preserves `OpenSubagent`. When expanded,
append:

```rust
self.render_workflow_progress(&row, theme)
```

No labels, models or statuses may be invented in the view.

- [ ] **Step 6: Run focused sidebar tests**

```bash
cargo test -p zeron-ui details_sidebar -- --nocapture
cargo test -p zeron-ui workers_widget -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Run final gates and mechanical detector**

```bash
cargo test -p zeron-ui
cargo fmt --all -- --check
git diff --check
node /Users/guilhermevarela/.agents/skills/impeccable/scripts/detect.mjs --json crates/ui/src/details_sidebar/view.rs crates/ui/src/details_sidebar/chat_workers.rs crates/ui/src/details_sidebar/widgets.rs
cargo build -p zeron
```

Expected: tests, formatting, detector and build pass with no unexplained new finding.

- [ ] **Step 8: Review and commit only sidebar parity files**

```bash
git add crates/ui/src/details_sidebar/chat_workers.rs crates/ui/src/details_sidebar/view.rs crates/ui/src/details_sidebar/widgets.rs
git commit -m "fix(ui): show native OMP subagent details"
```
