# OMP Todo Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render OMP's built-in todo lifecycle through Comet's shared `ToolCall::Todo` component while preserving OMP as the execution authority.

**Architecture:** Extend the stateful `OmpNormalizer` with the latest todo snapshot. Normalize todo starts from `list` or `items`, then reconcile authoritative `result.details.phases` snapshots on completion by re-emitting the same tool-call id before its result.

**Tech Stack:** Rust, `serde_json`, `zeron-proto`, existing OMP RPC normalizer tests.

**Spec:** `docs/plans/2026-08-23-omp-todo-parity-design.md`

## Global Constraints

- OMP remains the only todo executor and validator.
- Do not register a duplicate host tool or rewrite invalid payloads.
- Reuse `ToolCall::Todo` and the existing transcript stable-id replacement path.
- Preserve the original OMP error output and `is_error` flag.
- Do not touch the unrelated terminal-scroll working-tree changes.

---

### Task 1: Normalize OMP todo snapshots

**Files:**
- Modify: `crates/harness/src/omp/normalize.rs`
- Test: `crates/harness/src/omp/normalize.rs`

**Interfaces:**
- Consumes: OMP `tool_execution_start.args` and `tool_execution_end.result.details.phases` frames.
- Produces: `OmpNormalizer::push(Value) -> Vec<AgentEvent>` events containing `ToolCall::Todo { items: Vec<TodoItem> }` followed by the existing `ToolResult`.

- [ ] **Step 1: Write failing start-normalization tests**

Add focused tests that push phased and flattened init frames and expect:

```rust
AgentEvent::ToolCall {
    id: "todo-1".into(),
    call: ToolCall::Todo {
        items: vec![
            TodoItem { content: "Inspect state".into(), done: false },
            TodoItem { content: "Run gates".into(), done: false },
        ],
    },
}
```

Also push `{"op":"init","task":""}` and assert that it produces
`ToolCall::Todo { items: vec![] }`, not `ToolCall::Unknown`.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo test -p zeron-harness omp_todo -- --nocapture
```

Expected: FAIL because `normalize_tool("todo", ...)` currently returns
`ToolCall::Unknown`.

- [ ] **Step 3: Implement minimal start normalization**

Import `TodoItem`, add `todos: Vec<TodoItem>` to `OmpNormalizer`, and replace
the free start path with a state-aware method. Add helpers with these contracts:

```rust
fn todo_items_from_input(input: &Value) -> Option<Vec<TodoItem>>;
fn todo_items_from_phases(phases: &Value) -> Option<Vec<TodoItem>>;
```

`todo_items_from_input` must flatten `list[].items[]` in source order, accept
flat `items[]`, and return `Some(Vec::new())` for a todo operation without a
list so it still selects the shared renderer.

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run:

```bash
cargo test -p zeron-harness omp_todo -- --nocapture
```

Expected: all `omp_todo` tests pass.

- [ ] **Step 5: Write failing result-reconciliation tests**

Push a todo start followed by a successful result containing:

```json
{
  "details": {
    "phases": [{
      "name": "Work",
      "tasks": [
        {"content": "Inspect state", "status": "completed"},
        {"content": "Run gates", "status": "in_progress"}
      ]
    }]
  }
}
```

Assert the result frame emits an updated `ToolCall::Todo` with the same id
before `ToolResult`, mapping only `completed` and `abandoned` to `done: true`.
Add a failed-result case without phases and assert it preserves the last valid
snapshot while `ToolResult.is_error` remains true and its output contains
`Missing list for init operation`.

- [ ] **Step 6: Run result tests and verify RED**

Run:

```bash
cargo test -p zeron-harness omp_todo_result -- --nocapture
```

Expected: FAIL because result frames currently emit only `ToolResult` and the
normalizer retains no todo snapshot.

- [ ] **Step 7: Implement result reconciliation**

Make tool-end normalization state-aware. If authoritative phases exist, update
`self.todos`; for every todo result emit:

```rust
vec![
    AgentEvent::ToolCall {
        id: id.clone(),
        call: ToolCall::Todo { items: self.todos.clone() },
    },
    existing_tool_result,
]
```

For non-todo tools, preserve the current one-event behavior exactly.

- [ ] **Step 8: Run focused tests and verify GREEN**

Run:

```bash
cargo test -p zeron-harness omp_todo -- --nocapture
```

Expected: all focused tests pass.

- [ ] **Step 9: Run final gates**

Run:

```bash
cargo test -p zeron-harness
cargo test -p zeron-ui
cargo fmt --all -- --check
git diff --check
```

Expected: all tests and checks pass; only OMP normalizer files plus the already
documented unrelated working-tree paths are modified.

- [ ] **Step 10: Commit only the OMP parity change**

```bash
git add crates/harness/src/omp/normalize.rs
git commit -m "fix(omp): render todo snapshots through shared component"
```
