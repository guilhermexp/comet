# Chat Workers Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a chat-scoped Workers details widget with structured workflows, runtime subagents, and exact terminal-backed CLI worker sessions that open in the right split.

**Architecture:** Provider lifecycle frames normalize into additive `WorkflowTaskUpdate` events. Session documents retain those updates as non-visible activity parts, while existing subagent spawn chips remain the transcript source of truth. A read-only worker-parent binding snapshot joins CLI sessions to the selected chat. One GPUI presentation module feeds the Details widget and typed split actions.

**Tech Stack:** Rust, serde, Loro session documents, GPUI, and the existing `zeron-proto`, `zeron-harness`, `zeron-doc`, `zeron-engine`, `zeron-workers-unpeel`, and `zeron-ui` crates.

**Spec:** `docs/plans/2026-08-21-chat-workers-widget-design.md`

## Global Constraints

- Preserve the parent chat when a subagent or worker opens.
- Opening a worker attaches to the existing session; it never launches, restarts, stops, removes, or duplicates a PTY.
- Generic Bash/background tasks never appear as subagents.
- Missing terminal evidence never renders as success.
- Retain at most 100 settled activity rows in the widget projection; active rows are exempt.
- Preserve all existing Pi, Claude Code, Codex, ACP, and OMP behavior outside additive lifecycle metadata.
- Use existing theme, reduced-motion, provider-icon, Details-card, and right-pane conventions.

---

### Task 1: Normalized workflow activity protocol

**Files:**
- Modify: `crates/proto/src/agent.rs`

**Interfaces:**
- Produces: `WorkflowTaskStatus`, `WorkflowUsage`, `WorkflowProgressNode`, `WorkflowTaskUpdate`, and `AgentEvent::WorkflowTask { task }`.
- Consumes: existing `AgentEvent` serde conventions.

- [ ] **Step 1: Write the failing protocol test**

Add a backward-compatible minimal decode and a rich event round-trip:

```rust
#[test]
fn workflow_task_event_is_additive_and_round_trips() {
    let minimal: WorkflowTaskUpdate = serde_json::from_value(serde_json::json!({
        "taskId": "task-1", "status": "running"
    })).unwrap();
    assert_eq!(minimal.workflow_name, None);

    let event = AgentEvent::WorkflowTask {
        task: WorkflowTaskUpdate {
            task_id: "task-1".into(),
            status: WorkflowTaskStatus::Completed,
            workflow_name: Some("Audit".into()),
            description: Some("Review repository".into()),
            usage: Some(WorkflowUsage {
                total_tokens: Some(1_200),
                tool_uses: Some(4),
                duration_ms: Some(2_500),
            }),
            progress: vec![WorkflowProgressNode::Phase {
                index: 0,
                title: "Review".into(),
            }],
            agent_count: Some(1),
            task_type: Some("local_workflow".into()),
            subagent_type: None,
        },
    };
    let value = serde_json::to_value(&event).unwrap();
    assert_eq!(serde_json::from_value::<AgentEvent>(value).unwrap(), event);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-proto workflow_task_event_is_additive_and_round_trips`

Expected: compilation fails because the workflow types do not exist.

- [ ] **Step 3: Implement the protocol types**

Use camelCase serde and this shape:

```rust
pub enum WorkflowTaskStatus { Running, Completed, Failed, Cancelled }
pub struct WorkflowUsage {
    pub total_tokens: Option<u64>,
    pub tool_uses: Option<u64>,
    pub duration_ms: Option<u64>,
}
pub enum WorkflowProgressNode {
    Phase { index: u32, title: String },
    Agent {
        index: u32,
        label: String,
        phase_index: u32,
        phase_title: Option<String>,
        agent_id: Option<String>,
        model: Option<String>,
        state: Option<String>,
        prompt_preview: Option<String>,
    },
}
pub struct WorkflowTaskUpdate {
    pub task_id: String,
    pub status: WorkflowTaskStatus,
    pub workflow_name: Option<String>,
    pub description: Option<String>,
    pub usage: Option<WorkflowUsage>,
    pub progress: Vec<WorkflowProgressNode>,
    pub agent_count: Option<u32>,
    pub task_type: Option<String>,
    pub subagent_type: Option<String>,
}
```

- [ ] **Step 4: Verify GREEN and commit**

Run: `cargo test -p zeron-proto agent::tests`

```bash
git add crates/proto/src/agent.rs
git commit -m "feat(proto): add workflow activity events"
```

### Task 2: Claude Code and OMP lifecycle normalization

**Files:**
- Modify: `crates/harness/src/claude/wire.rs`
- Modify: `crates/harness/src/claude/normalize.rs`
- Modify: `crates/harness/src/omp/normalize.rs`

**Interfaces:**
- Consumes: Task 1 protocol types.
- Produces: provider-independent workflow events without changing nested subagent transcript routing.

- [ ] **Step 1: Write failing Claude tests**

Cover a `task_progress` frame with workflow name, type, usage, phases, and agents; a sparse terminal `task_notification`; and a `local_bash` task that must not emit subagent activity.

```rust
assert!(matches!(&events[..], [AgentEvent::WorkflowTask { task }]
    if task.task_id == "wf-1" && task.workflow_name.as_deref() == Some("Audit")));
assert!(!bash_events.iter().any(|event| matches!(event, AgentEvent::WorkflowTask { .. })));
```

- [ ] **Step 2: Write failing OMP tests**

Feed `subagent_lifecycle` and `subagent_progress` frames. Assert one stable task ID, `task_type == "subagent"`, provider agent name, usage, resolved model, and terminal status while existing `AgentEvent::Subagent` frames still emit.

- [ ] **Step 3: Verify RED**

Run: `cargo test -p zeron-harness claude_task_progress_emits_workflow_activity omp_subagent_progress_emits_workflow_activity`

Expected: activity frames are absent.

- [ ] **Step 4: Implement tolerant provider mappings**

Extend `SystemFrame` with optional `description`, `workflow_name`, `usage`, `workflow_progress`, `task_type`, `patch`, and `summary`. Map only `task_started`, `task_updated`, `task_progress`, and `task_notification`. Parse `<agent_count>N</agent_count>` when present. In OMP, handle `subagent_progress` and inherit sparse metadata from the existing per-subagent context. Malformed optional metadata is ignored, not fatal.

- [ ] **Step 5: Verify GREEN and commit**

Run: `cargo test -p zeron-harness claude::normalize::tests omp::normalize::tests`

```bash
git add crates/harness/src/claude/wire.rs crates/harness/src/claude/normalize.rs crates/harness/src/omp/normalize.rs
git commit -m "feat(harness): normalize workflow activity"
```

### Task 3: Durable activity merge and bounded projection

**Files:**
- Modify: `crates/doc/src/parts.rs`
- Modify: `crates/doc/src/schema.rs`
- Modify: `crates/engine/src/sessions.rs`

**Interfaces:**
- Consumes: `AgentEvent::WorkflowTask`.
- Produces: invisible `MessagePart::WorkflowTask`, sparse merge semantics, and `workflow_tasks_from_entries(entries, settled_limit)`.

- [ ] **Step 1: Write failing merge and bound tests**

```rust
#[test]
fn workflow_updates_preserve_richer_fields() {
    let mut parts = Vec::new();
    fold_event_into_parts(&mut parts, &rich_running_workflow("wf-1"));
    fold_event_into_parts(&mut parts, &sparse_completed_workflow("wf-1"));
    let MessagePart::WorkflowTask { task } = &parts[0] else { panic!("workflow") };
    assert_eq!(task.status, WorkflowTaskStatus::Completed);
    assert_eq!(task.workflow_name.as_deref(), Some("Audit"));
    assert_eq!(task.progress.len(), 2);
}
```

Add 102 settled tasks and two active tasks across entries. Assert the projection returns the newest 100 settled tasks plus both active tasks in stable document order.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-doc workflow_updates_preserve_richer_fields workflow_projection_bounds_only_settled_history`

Expected: the part and helpers do not exist.

- [ ] **Step 3: Implement activity persistence**

Add a non-rendered `MessagePart::WorkflowTask { task: WorkflowTaskUpdate }`. Extend part IDs, byte sizing, JSON conversion, schema read/write, and in-place field refresh. `fold_event_into_parts` merges by task ID inside the active segment: new non-empty metadata replaces old values, absent metadata preserves old values, progress merges by phase/agent identity, and status always advances to the latest event.

- [ ] **Step 4: Implement bounded projection**

`workflow_tasks_from_entries` walks newest-to-oldest, deduplicates task IDs, keeps every active task, keeps at most `settled_limit` settled tasks, then restores stable chronological order. It does not mutate the transcript.

- [ ] **Step 5: Verify engine compatibility**

Workflow parts must persist through the normal segment writer, remain invisible in transcript rows, not split reasoning, and not affect unresolved-tool quiescence.

Run:

```bash
cargo test -p zeron-doc parts::tests schema::tests
cargo test -p zeron-engine sessions::tests
```

- [ ] **Step 6: Commit**

```bash
git add crates/doc/src/parts.rs crates/doc/src/schema.rs crates/engine/src/sessions.rs
git commit -m "feat(doc): persist chat workflow activity"
```

### Task 4: Read-only worker parent links and presentation projection

**Files:**
- Modify: `crates/workers-unpeel/src/parent_notifications.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/tests/controller_mcp.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Create: `crates/ui/src/details_sidebar/chat_workers.rs`
- Modify: `crates/ui/src/details_sidebar/mod.rs`

**Interfaces:**
- Produces: `WorkerParentLink`, `worker_parent_links()`, `WorkersModel::sessions_for_parent_chat`, `ChatWorkersSnapshot`, and pure row/status projection.
- Consumes: persisted parent bindings, `WorkersBootstrap.sessions`, transcript subagent chips, and Task 3 workflow projection.

- [ ] **Step 1: Write failing binding tests**

Register two sessions to different chat IDs in a temporary app-state file. Assert the public snapshot exposes only session ID, parent chat ID, and registration timestamp in deterministic order.

- [ ] **Step 2: Write failing projection tests**

Cover category separation, generic background-task exclusion, active-first stable ordering, exact chat filtering, and exhaustive worker semantics including distinct recovery/disconnected states.

Define test-only fixture constructors in the same test module with these exact
signatures: `workflow_task(id: &str) -> WorkflowTaskUpdate`,
`subagent_task(id: &str) -> WorkflowTaskUpdate`, and
`worker_session(id: &str, state: &str, activity: &str) -> WorkersSession`.
They return otherwise-minimal valid values and never enter production code.

```rust
#[test]
fn projection_separates_workflows_subagents_and_workers() {
    let snapshot = project_chat_workers(
        vec![workflow_task("wf-1"), subagent_task("sub-1")],
        vec![worker_session("worker-1", "running", "working")],
    );
    assert_eq!(snapshot.workflows.len(), 1);
    assert_eq!(snapshot.subagents.len(), 1);
    assert_eq!(snapshot.workers.len(), 1);
}

#[test]
fn worker_semantics_do_not_infer_success() {
    assert_eq!(worker_semantic("running", "idle"), WorkerSemantic::Idle);
    assert_eq!(worker_semantic("exited", "idle"), WorkerSemantic::Recovery);
    assert_ne!(worker_semantic("exited", "idle"), WorkerSemantic::Terminal);
}
```

- [ ] **Step 3: Verify RED**

Run:

```bash
cargo test -p zeron-workers-unpeel worker_parent_links_are_read_only
cargo test -p zeron-ui details_sidebar::chat_workers::tests
```

- [ ] **Step 4: Implement the binding snapshot**

Expose:

```rust
pub struct WorkerParentLink {
    pub worker_session_id: String,
    pub parent_chat_id: String,
    pub registered_at_unix_ms: u64,
}
```

Do not expose mutation, task episodes, acknowledgements, or notification evidence. `WorkersModel::sessions_for_parent_chat` joins links to live sessions and ignores stale session IDs without leaking another chat's workers.

- [ ] **Step 5: Implement pure presentation types**

Classify workflows when they have `task_type == "local_workflow"`, a workflow name, multiple agents, or phase nodes. Classify subagents only from `subagent_type` or durable spawn-chip identity. Format tokens/tools/duration compactly. Map worker states exhaustively: starting/working, blocked, terminal, idle, recovery, disconnected.

- [ ] **Step 6: Verify GREEN and commit**

Run:

```bash
cargo test -p zeron-workers-unpeel worker_parent
cargo test -p zeron-ui details_sidebar::chat_workers::tests workers::model::tests
```

```bash
git add crates/workers-unpeel/src/parent_notifications.rs crates/workers-unpeel/src/lib.rs crates/workers-unpeel/tests/controller_mcp.rs crates/ui/src/workers/model.rs crates/ui/src/details_sidebar/chat_workers.rs crates/ui/src/details_sidebar/mod.rs
git commit -m "feat(ui): project chat worker activity"
```

### Task 5: Details widget UI and typed actions

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/src/details_sidebar/widgets.rs`
- Modify: `crates/ui/src/icons.rs`
- Modify: `crates/ui/src/shell.rs`

**Interfaces:**
- Produces: `ChatWorkersTab`, local row expansion, `DetailsSidebarEvent::OpenSubagent`, and `DetailsSidebarEvent::OpenWorkerSession`.
- Consumes: `ChatWorkersSnapshot`, `WorkersModel`, and existing Details card/tokens.

- [ ] **Step 1: Write failing widget-state tests**

Test first-non-empty auto-selection, explicit selection persistence, independent workflow expansion, and context-key reset:

```rust
#[test]
fn workers_widget_auto_selects_first_non_empty_tab() {
    assert_eq!(auto_tab(2, 1, 3), ChatWorkersTab::Workflows);
    assert_eq!(auto_tab(0, 1, 3), ChatWorkersTab::Subagents);
    assert_eq!(auto_tab(0, 0, 3), ChatWorkersTab::Workers);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-ui workers_widget_auto_selects_first_non_empty_tab`

- [ ] **Step 3: Inject and observe WorkersModel**

Change `DetailsSidebar::new(app_state, workers_model, preferences, cx)`. Observe both entities, rebuild the selected chat projection on transcript or worker lifecycle changes, and reset widget-local state only when the Details context key changes.

- [ ] **Step 4: Render the card**

Place it after Workspace. Match the reference card header, three-tab strip, counts, selected ink fill, five-row scroll body, active-first order, workflow phase tree, status glyphs, empty selected-tab message, provider icons, and reduced-motion behavior. Hide the whole card only when all categories are empty.

- [ ] **Step 5: Emit typed actions**

Subagent rows emit chat/doc/title/frozen identity through the existing subagent-open seam. Worker rows emit stable session ID and title. Missing/stale targets render disabled and emit nothing.

- [ ] **Step 6: Verify GREEN and commit**

Run: `cargo test -p zeron-ui details_sidebar`

```bash
git add crates/ui/src/details_sidebar/view.rs crates/ui/src/details_sidebar/widgets.rs crates/ui/src/icons.rs crates/ui/src/shell.rs
git commit -m "feat(ui): add chat workers details widget"
```

### Task 6: Exact worker terminal split

**Files:**
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/workers/terminal.rs`

**Interfaces:**
- Produces: `RightSurface::Worker(u64)`, `WorkerTerminalTab`, `register_worker_surface`, and `add_worker_surface`.
- Consumes: `DetailsSidebarEvent::OpenWorkerSession`, `WorkersTerminal::new`, and `WorkersTerminal::set_session`.

- [ ] **Step 1: Write failing surface identity tests**

```rust
#[test]
fn worker_surface_reuses_exact_session_identity() {
    let (first, created) = register_worker_surface(&mut surfaces, &mut seq, "session-1", "Audit");
    assert!(created);
    let (second, created) = register_worker_surface(&mut surfaces, &mut seq, "session-1", "Renamed");
    assert!(!created);
    assert_eq!(first, second);
}
```

Add a terminal test proving view detach performs no stop/restart/remove client operation.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p zeron-ui worker_surface_reuses_exact_session_identity workers_terminal_detach_is_view_only`

- [ ] **Step 3: Add right-surface storage and behavior**

Store one `Entity<WorkersTerminal>` per stable worker session ID. First open creates the entity, sets the exact session, adds the tab, and focuses it. Repeat open focuses the same tab. Close drops only the view entity. Resolve the session against the latest Workers snapshot immediately before opening; if absent, refresh the model and leave the current surface unchanged.

- [ ] **Step 4: Render worker surfaces and tabs**

Render the worker terminal full-size through `TerminalElement::new_workers`. Include worker surfaces in active resolution, tab ordering/reordering, labels, close, right-pane width, and cleanup. Preserve the selected parent chat throughout.

- [ ] **Step 5: Verify GREEN and commit**

Run: `cargo test -p zeron-ui shell::tests workers::terminal::tests`

```bash
git add crates/ui/src/shell.rs crates/ui/src/workers/terminal.rs
git commit -m "feat(ui): open workers in chat split"
```

### Task 7: Full gates, visual parity, and review

**Files:**
- Modify only evidence-backed files from Tasks 1-6.

**Interfaces:**
- Consumes: complete widget and split behavior.
- Produces: focused/full green gates and real native visual evidence.

- [ ] **Step 1: Run focused suites**

```bash
cargo test -p zeron-proto agent::tests
cargo test -p zeron-harness claude::normalize::tests omp::normalize::tests
cargo test -p zeron-doc parts::tests schema::tests
cargo test -p zeron-workers-unpeel worker_parent
cargo test -p zeron-ui details_sidebar shell::tests workers::terminal::tests
```

- [ ] **Step 2: Run canonical workspace gates once**

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo build -p zeron
git diff --check
```

- [ ] **Step 3: Validate the real app side by side**

After checking active workers before any restart, exercise one running/completed subagent, one available structured workflow, and one CLI worker created through the Workers MCP. Verify counts, active-first order, phase expansion, subagent split, exact terminal split, repeat-click focus, view-only close, and recovery/disconnected semantics. Compare card spacing, tabs, scroll height, icons, and status colors directly with Orchestrator.dev. A green build alone does not satisfy this gate.

- [ ] **Step 4: Correct only observed gaps with RED first**

For each mismatch, add a focused failing regression test, apply the smallest correction, and rerun that test. Stop and report after three unsuccessful attempts on the same gap.

- [ ] **Step 5: Request code review**

Use `requesting-code-review` on the complete diff. Resolve every Critical and Important finding with a regression test before the final gate.

- [ ] **Step 6: Final gate and commit**

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo build -p zeron
git diff --check
git status --short
git add -A
git commit -m "feat(ui): replicate chat workers widget"
```

Expected: clean worktree. Do not push or merge without separate authorization.
