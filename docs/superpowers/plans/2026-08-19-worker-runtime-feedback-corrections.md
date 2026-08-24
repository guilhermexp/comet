# Worker Runtime Feedback Corrections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver exactly one parent notification per real delegated task, migrate managed runtime hooks from Unpeel paths to Comet-owned paths, and make Workers controller feedback reliable and readable.

**Architecture:** The controller owns a durable delegation episode. Provider hooks are evidence inside that episode, not task identity. Completion is gated by settled output and task-owned process quiescence. Hook assets live under `~/.zeron/workers/hooks`; provider configuration is reconciled before legacy hook files are removed.

**Tech Stack:** Rust 2024, GPUI, vendored Unpeel runtime adapters, JSON/JSONL durable state, macOS process inspection, terminal viewport snapshots.

## Global Constraints

- Preserve unrelated user hooks and all existing Worker sessions.
- Do not commit, push, reset, or delete repository work without explicit authorization.
- TDD every behavior change: focused RED, minimal GREEN, then refactor.
- Fail closed when completion evidence or hook migration verification is unavailable.

---

### Task 1: Durable delegation episodes

**Files:**
- Modify: `crates/workers-unpeel/src/parent_notifications.rs`
- Modify: `crates/workers-unpeel/src/controller_mcp.rs`
- Modify: `crates/workers-unpeel/tests/parent_notifications.rs`
- Modify: `crates/workers-unpeel/tests/controller_mcp.rs`

**Interfaces:**
- Produces: durable binding fields `active_task_episode`, `submitted_at_unix_ms`, and `acknowledged_completed_episode`.
- Produces: `begin_worker_task_episode(session_id, parent_chat_id, submitted_at)` called only after successful submission.

- [ ] Add a failing reducer test where repeated Start/Stop events in one episode produce one completion.
- [ ] Add a failing test where a controller-submitted follow-up increments the episode and permits one later completion.
- [ ] Implement episode persistence and remove provider Start as a completion rearm signal.
- [ ] Run `cargo test -p zeron-workers-unpeel --test parent_notifications` and confirm green.

### Task 2: Background-process and output-quiescence completion gate

**Files:**
- Modify: `crates/workers-unpeel/src/parent_notifications.rs`
- Modify: `crates/workers-unpeel/src/resources.rs`
- Modify: `crates/workers-unpeel/src/session_event_journal.rs`
- Modify: `crates/workers-unpeel/tests/parent_notifications.rs`

**Interfaces:**
- Produces: `WorkerCompletionEvidence { output_modified_unix_ms, has_task_owned_processes, inspection_complete }`.
- Consumes: session root PID/start time and the process snapshot already used by resource attribution.

- [ ] Add a failing test: Stop plus a task-owned child process is not completed.
- [ ] Add a failing test: the same episode completes once after the child exits and output is quiescent.
- [ ] Expose deterministic process-delta evaluation from `resources.rs` and persist the episode baseline.
- [ ] Gate completion on successful inspection and output quiescence.
- [ ] Run the focused notification and resource tests.

### Task 3: Comet-owned hook root and safe migration

**Files:**
- Modify: `third_party/unpeel/crates/unpeel-core/src/app_paths.rs`
- Modify: `third_party/unpeel/crates/unpeel-core/src/hook_assets/mod.rs`
- Modify: runtime adapter setup files under `third_party/unpeel/runtimes/*/adapter/setup.rs`
- Modify: lifecycle assets under `third_party/unpeel/runtimes/*/assets/hooks/`
- Add: `crates/workers-unpeel/src/hook_migration.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Add: `crates/workers-unpeel/tests/hook_migration.rs`

**Interfaces:**
- Produces: `app_hooks_root() -> ~/.zeron/workers/hooks`.
- Produces: `migrate_managed_hooks() -> Result<HookMigrationReport, String>`.

- [ ] Add failing path tests proving managed assets resolve outside `~/.unpeel/hooks`.
- [ ] Add failing config tests for `~/.unpeel`, `/tmp`, `/private/tmp`, and `/var/folders` stale hooks while preserving unrelated hooks.
- [ ] Centralize every managed hook path on the Comet hook root.
- [ ] Install and verify new assets/config references before deleting the legacy hook directory.
- [ ] Run runtime setup conformance and Workers migration tests.

### Task 4: Semantic output and activity timestamps

**Files:**
- Modify: `crates/workers-unpeel/src/controller_mcp.rs`
- Modify: `crates/workers-unpeel/src/activity_bridge.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/tests/controller_mcp.rs`
- Modify: activity bridge unit tests.

**Interfaces:**
- Produces: semantic rendered-screen output for controller `read_output` and `inspect_worker`.
- Produces: `updated_at_unix_ms = max(manifest, output mtime, hook mtime)`.

- [ ] Add a failing spinner-repaint test that preserves useful final screen text.
- [ ] Verify the existing viewport projection is used by both controller actions and fix only remaining fallback leakage.
- [ ] Add a failing timestamp test with growing `output.bin` and unchanged manifest.
- [ ] Implement the timestamp maximum and run focused tests.

### Task 5: Reliable launch and brief submission

**Files:**
- Modify: `crates/workers-unpeel/src/controller_mcp.rs`
- Modify: `crates/workers-unpeel/src/workspace_trust.rs`
- Modify: `crates/workers-unpeel/tests/controller_mcp.rs`
- Modify: `crates/workers-unpeel/tests/workspace_trust.rs`

**Interfaces:**
- Produces: bounded `submit_initial_briefing` result that starts an episode only after submission.
- Produces: structured partial-launch error containing the created session ID.

- [ ] Add a failing test for a first-run/update prompt before the real agent prompt.
- [ ] Add a failing test that submission failure preserves the session ID and creates no task episode.
- [ ] Reuse deterministic menu/trust detection to reach the agent input, then submit.
- [ ] Start the parent binding/episode only after confirmed write and run focused tests.

### Task 6: Verification and live migration

**Files:**
- Modify: the design/plan checkboxes only after evidence exists.

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo test -p zeron-workers-unpeel`.
- [ ] Run focused UI notification tests.
- [ ] Run `cargo check --workspace` once.
- [ ] Build `cargo build -p zeron`.
- [ ] Stop/restart only the exact dev main process after checking active Workers; never use `pkill -x zeron`.
- [ ] Verify provider configs reference only `~/.zeron/workers/hooks`, then remove `~/.unpeel/hooks`.
- [ ] Reproduce one delegated task with an active background process and confirm zero premature notifications, followed by one final notification.
