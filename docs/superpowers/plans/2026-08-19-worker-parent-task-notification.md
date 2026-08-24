# Worker Parent Task Notification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resume the exact parent Orchestrator chat once for every actionable lifecycle event emitted by a CLI Worker it launched.

**Architecture:** Thread the authoritative parent chat ID into the controller MCP, persist Worker-to-parent bindings in guarded Unpeel state, capture every lifecycle episode in a detached-host-owned append-only journal, and deliver them through a synchronously persisted `QueueWorkerNotification` command with deterministic IDs.

**Tech Stack:** Rust, serde, GPUI entities/tasks, ACP stdio MCP, Loro-backed session command ledger, Unpeel hosted-session manifests.

## Global Constraints

- Use the existing session command ledger as the only durable notification outbox.
- Never inject an unbounded or raw terminal transcript into the parent agent.
- UI-created Workers without a parent binding remain unaffected.
- Worker descendants receive neither controller authority nor parent chat identity.
- Lifecycle history survives GPUI downtime and distinguishes episodes by a durable sequence.
- Reconcile async-provider hook gaps and compact acknowledged history without losing latch context.
- Every production behavior is introduced by a failing test first.

---

### Task 1: Parent chat identity reaches only the controller MCP

**Files:**
- Modify: `crates/proto/src/agent.rs`
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/harness/src/acp/mod.rs`
- Test: `crates/proto/src/agent.rs`
- Test: `crates/harness/tests/acp.rs`

**Interfaces:**
- Produces: `RunRequest::workers_parent_chat_id: Option<String>`.
- Produces: `workers_mcp_servers_for(executable, enabled, disabled, parent_chat_id)`.
- Consumes: the authoritative `chat_id` already owned by `SessionsEngine::dispatch_inner`.

- [x] **Step 1: Write failing serialization and descriptor tests**

Add assertions that the optional field defaults to `None`, round-trips as
`workersParentChatId`, and appears as `COMET_WORKERS_PARENT_CHAT_ID` only in an
enabled controller MCP descriptor.

- [x] **Step 2: Run the focused tests and verify RED**

Run: `cargo test -p zeron-proto && cargo test -p zeron-harness --test acp workers_mcp`

Expected: compilation or assertion failure because the field and descriptor
argument do not exist.

- [x] **Step 3: Implement the minimal identity propagation**

Add the serde-defaulted field, stamp it from `chat_id` immediately before the
harness request is stored/spawned, and pass it into the controller descriptor as
a child-only environment value.

- [x] **Step 4: Run focused tests and verify GREEN**

Run: `cargo test -p zeron-proto && cargo test -p zeron-harness --test acp workers_mcp`

Expected: PASS.

### Task 2: Persist bindings and journal lifecycle episodes

**Files:**
- Create: `crates/workers-unpeel/src/parent_notifications.rs`
- Create: `crates/workers-unpeel/src/session_event_journal.rs`
- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/src/controller_mcp.rs`
- Test: `crates/workers-unpeel/tests/parent_notifications.rs`
- Test: `crates/workers-unpeel/tests/controller_mcp.rs`

**Interfaces:**
- Produces: `register_worker_parent(session_id, parent_chat_id, registered_at)`.
- Produces: `pending_worker_parent_notifications(sessions)` returning generation-scoped notification IDs and parent chat IDs.
- Produces: `ack_worker_parent_notification(notification)`.
- Produces: bounded `worker_notification_output_tail(session_id)`.

- [x] **Step 1: Write failing binding and reducer tests**

Cover PermissionRequest/Stop/exited precedence, repeated Start-to-Stop episodes,
multiple episodes accumulated during app downtime, terminal acknowledgement,
manual Workers, malformed state, and bounded ANSI/control-free output.

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p zeron-workers-unpeel --test parent_notifications`

Expected: FAIL because the module and public API do not exist.

- [x] **Step 3: Implement the detached-host journal and guarded reducer**

Give every detached session host a private loopback hook endpoint, append each
accepted event synchronously to `comet-hook-events.jsonl`, and store bindings
under `comet_worker_parent_notifications` using `unpeel_core::app_state::edit`.
Derive stable IDs from session ID, runtime generation and journal sequence.

- [x] **Step 4: Bind successful controller launches**

Capture and remove `COMET_WORKERS_PARENT_CHAT_ID` during controller startup.
Register the new Worker immediately after session creation, before briefing
delivery, so a partially launched but later recovered Worker remains linked.

- [x] **Step 5: Run focused tests and verify GREEN**

Run: `cargo test -p zeron-workers-unpeel --test parent_notifications && cargo test -p zeron-workers-unpeel --test controller_mcp`

Expected: PASS.

### Task 3: Make queued notification commands deterministically idempotent

**Files:**
- Modify: `crates/engine/src/rpc.rs`
- Modify: `crates/engine/src/doc_host.rs`
- Test: `crates/engine/tests/e2e.rs`

**Interfaces:**
- Produces: `QueueWorkerNotification` RPC with required deterministic ID.
- Produces: `DocHost::queue_worker_notification(chat_id, command_id, payload)`.
- Preserves: ordinary `QueueCommand` behavior unchanged.

- [x] **Step 1: Write a failing duplicate-command integration test**

Queue the same deterministic ID twice and assert the command ledger executes the
Worker notification prompt once while both calls return the same ID.

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p zeron-engine --test e2e deterministic_queue_command_id`

Expected: FAIL because `QueueCommand` ignores caller-provided IDs.

- [x] **Step 3: Implement a dedicated deterministic notification RPC**

Validate the ID and `Steer` payload, reject a missing parent chat, enqueue under
the deterministic ID, and persist the chat snapshot before RPC success.

- [x] **Step 4: Run focused tests and verify GREEN**

Run: `cargo test -p zeron-engine --test e2e deterministic_queue_command_id`

Expected: PASS.

### Task 4: Deliver pending Worker events to the parent chat

**Files:**
- Modify: `crates/ui/src/lib.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Test: `crates/ui/src/workers/model.rs`

**Interfaces:**
- Consumes: `pending_worker_parent_notifications` and `AppState::engine()`.
- Consumes: `QueueWorkerNotification { chatId, commandId, command: Steer }`.
- Produces: one in-flight RPC attempt per stable notification ID.

- [x] **Step 1: Write failing prompt and delivery-state tests**

Test the fixed prompt framing, deterministic command/message IDs, no duplicate
in-flight attempt, success acknowledgement, and failure retry behavior through a
small pure delivery reducer.

- [x] **Step 2: Run the focused UI tests and verify RED**

Run: `cargo test -p zeron-ui worker_parent_notification`

Expected: FAIL because the coordinator helpers do not exist.

- [x] **Step 3: Implement the minimal coordinator**

Construct `WorkersModel` with the shared `AppState`, inspect pending events after
each snapshot, read a bounded output tail, and queue a deterministic `Steer`.
Ack only after `QueueWorkerNotification` succeeds; retain failures for a later refresh.

- [x] **Step 4: Run focused tests and verify GREEN**

Run: `cargo test -p zeron-ui worker_parent_notification`

Expected: PASS.

### Task 5: Integration and regression gates

**Files:**
- Modify: `docs/plans/2026-08-19-worker-parent-task-notification-design.md` only if implementation evidence changes the contract.

**Interfaces:**
- Verifies all interfaces produced by Tasks 1-4 together.

- [x] **Step 1: Run focused subsystem suites**

Run: `cargo test -p zeron-workers-unpeel && cargo test -p zeron-harness --test acp && cargo test -p zeron-engine --test e2e && cargo test -p zeron-ui worker_parent_notification`

Expected: PASS.

- [x] **Step 2: Run repository gates**

Run: `cargo fmt --all -- --check && cargo check --workspace && git diff --check`

Expected: PASS with no formatting or whitespace errors.

- [ ] **Step 3: Perform native dev validation**

Launch one Worker from a primary Orchestrator chat, let it finish, verify that
the same chat receives one `[worker-task-notification]` turn, restart the app,
and verify the event is not sent again.
