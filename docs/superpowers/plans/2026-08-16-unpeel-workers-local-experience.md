# Unpeel Workers Local Experience Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Comet's local Workers mode with the Unpeel project/session tree, launcher, terminal I/O, lifecycle actions, archives, and visual parity while preserving Orchestrator behavior.

**Architecture:** `zeron-workers-unpeel` remains the only boundary to Unpeel core and exposes typed operations over its Controller/Host routes. A GPUI `WorkersModel` owns polling, selection, expanded projects, actions, and errors. `WorkersSidebar` and `WorkersContent` are independent GPUI entities observing that model, so Shell only switches surfaces and Orchestrator state stays untouched.

**Tech Stack:** Rust 2024, GPUI, alacritty_terminal, pinned Unpeel core 0.2.1, serde, base64, Cargo tests, native macOS visual smoke.

## Global Constraints

- Keep Orchestrator mode behavior unchanged.
- Use canonical `UNPEEL_HOME` or `~/.unpeel` through official Unpeel routes.
- Keep Unpeel upstream code unchanged under `third_party/unpeel`.
- Current scope is the local Mac; Link, iOS, billing, licensing, LAN, and SSH remain deferred.
- Use Comet identity and existing theme; do not show Unpeel branding.
- Every behavior starts with a failing test and reaches GREEN before the next behavior.
- Visual verification uses the real GPUI app and a controlled Workers fixture.

---

## Task 1: Complete the typed Workers client

**Files:**

- Modify: `crates/workers-unpeel/src/lib.rs`
- Modify: `crates/workers-unpeel/tests/local_bootstrap.rs`
- Create: `crates/workers-unpeel/tests/local_actions.rs`

**Interfaces:**

- Produces: `WorkersBootstrap`, `WorkersProject`, `WorkersPreset`, `WorkersSession`, `WorkersOutput`, `SessionAction`.
- Produces methods: `bootstrap`, `read_output`, `write`, `resize`, `create_session`, `session_action`, `set_session_organization`, `archived_sessions`.

- [ ] Write RED tests for the complete bootstrap DTO, output offset decoding, malformed responses, and validation of mutation request bodies.
- [ ] Run `cargo test -p zeron-workers-unpeel` and confirm the new APIs are missing.
- [ ] Implement a private request router with unique request ids and typed errors; all public methods call `ControllerHostRuntime::handle_tunnel`.
- [ ] Run `cargo test -p zeron-workers-unpeel` and confirm all tests pass.
- [ ] Commit with `feat(workers): complete local Unpeel client`.

The public operation signatures are:

```rust
pub fn bootstrap(&self) -> Result<WorkersBootstrap, WorkersError>;
pub fn read_output(&self, session_id: &str, offset: Option<u64>, wait_ms: u64)
    -> Result<WorkersOutput, WorkersError>;
pub fn write(&self, session_id: &str, data: &str) -> Result<(), WorkersError>;
pub fn resize(&self, session_id: &str, columns: u16, rows: u16)
    -> Result<(), WorkersError>;
pub fn create_session(&self, project_id: &str, command: &str)
    -> Result<String, WorkersError>;
pub fn session_action(&self, session_id: &str, action: SessionAction)
    -> Result<(), WorkersError>;
pub fn set_session_organization(&self, session_id: &str, patch: SessionOrganizationPatch)
    -> Result<(), WorkersError>;
pub fn archived_sessions(&self, project_id: &str)
    -> Result<Vec<WorkersSession>, WorkersError>;
```

---

## Task 2: Add the reactive Workers model

**Files:**

- Create: `crates/ui/src/workers/mod.rs`
- Create: `crates/ui/src/workers/model.rs`
- Modify: `crates/ui/src/lib.rs`
- Modify: `crates/ui/Cargo.toml`

**Interfaces:**

- Consumes: `LocalWorkersClient` and all Task 1 DTOs.
- Produces: `WorkersModel::new`, `select_session`, `toggle_project`, `refresh`, `launch`, `write`, `resize`, `stop`, `restart`, `pin`, `archive`, `restore`.

- [ ] Write RED unit tests for initial selection, stable selection after refresh, project grouping, and expanded-project toggles.
- [ ] Run `cargo test -p zeron-ui workers::model` and confirm the model is missing.
- [ ] Implement pure reducers first, then the GPUI entity with a one-second background refresh and explicit immediate refresh after actions.
- [ ] Ensure stale async results cannot replace a newer snapshot by comparing refresh generations.
- [ ] Run `cargo test -p zeron-ui workers::model` and confirm GREEN.
- [ ] Commit with `feat(workers): add reactive local model`.

---

## Task 3: Port the Workers sidebar and launcher

**Files:**

- Create: `crates/ui/src/workers/sidebar.rs`
- Create: `crates/ui/src/workers/content.rs`
- Modify: `crates/ui/src/workers/mod.rs`
- Modify: `crates/ui/src/shell.rs`

**Interfaces:**

- Consumes: `Entity<WorkersModel>`.
- Produces: `WorkersSidebar` and `WorkersContent`, both GPUI `Render` entities.

- [ ] Write RED tests for status visual mapping, relative age labels, session grouping, and empty-state copy.
- [ ] Run `cargo test -p zeron-ui workers::sidebar workers::content` and confirm RED.
- [ ] Render project rows at 28px with folder icon, disclosure state, status/unread indicators, hover actions, and selected session wash.
- [ ] Render the footer with refresh and add-session affordances; show a Comet-branded Add Project empty state when no projects exist.
- [ ] Render the main empty state and the preset/command launcher for the selected project.
- [ ] Change Shell so Workers mode mounts WorkersSidebar and WorkersContent; Orchestrator route/settings rendering remains byte-for-byte on its branch.
- [ ] Add `ZERON_SIDEBAR_MODE=workers` for deterministic visual boot without changing the default.
- [ ] Run focused tests and `cargo check -p zeron-ui`.
- [ ] Commit with `feat(workers): port project and session workspace`.

---

## Task 4: Add live terminal I/O

**Files:**

- Create: `crates/ui/src/workers/terminal.rs`
- Modify: `crates/ui/src/workers/content.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Reuse: `crates/ui/src/terminal/emulator.rs`
- Reuse: `crates/ui/src/terminal/view.rs`

**Interfaces:**

- Consumes: selected live `WorkersSession` and `LocalWorkersClient` operations.
- Produces: `WorkersTerminal`, feeding bytes into `Emulator`, forwarding keyboard/paste, resizing, and polling from `next_offset`.

- [ ] Write RED tests for output cursor advancement, truncation reset, terminal session switching, and input encoding.
- [ ] Run `cargo test -p zeron-ui workers::terminal` and confirm RED.
- [ ] Implement the output loop with long-polling off the GPUI render thread and generation cancellation on session switch.
- [ ] Render the terminal grid with the existing Comet terminal palette and 16/8px Unpeel viewport padding.
- [ ] Forward keyboard input through `keystroke_bytes`, paste through `paste_bytes`, and measured dimensions through `resize`.
- [ ] Render exited-session actions and loading/error states without terminal chrome.
- [ ] Run focused tests, adapter tests, and `cargo check -p zeron-ui`.
- [ ] Commit with `feat(workers): add live Unpeel terminal`.

---

## Task 5: Complete local lifecycle and library surfaces

**Files:**

- Modify: `crates/ui/src/workers/sidebar.rs`
- Modify: `crates/ui/src/workers/content.rs`
- Modify: `crates/ui/src/workers/model.rs`
- Create: `crates/ui/src/workers/archive.rs`

- [ ] Write RED tests for pin/archive/restore/restart state transitions and archived grouping.
- [ ] Add session context actions for pin, rename, archive, stop, restart, and remove with destructive confirmation.
- [ ] Add project-scoped archived-session library with restore, resume/restart, and permanent removal.
- [ ] Add inline error notice and loading skeletons; errors never clear the last good snapshot.
- [ ] Run focused tests and the complete `zeron-ui` test subset.
- [ ] Commit with `feat(workers): complete local session lifecycle`.

---

## Task 6: Package, test, and visually verify

**Files:**

- Modify only if required: app packaging scripts and license notices.
- Create: `.impeccable/review/workers-local.png`

- [ ] Build `unpeel-host` from the pinned workspace and verify its runtime assets are reachable in development.
- [ ] Run `cargo fmt --all -- --check`, `cargo test -p zeron-workers-unpeel`, `cargo test -p zeron-ui workers`, `cargo test -p zeron-ui sidebar_mode`, `cargo check -p zeron-ui`, and `cargo build -p zeron`.
- [ ] Create an isolated `UNPEEL_HOME` fixture with two projects and representative live/exited sessions.
- [ ] Start Comet with `ZERON_SIDEBAR_MODE=workers`, open the real macOS window, and capture `.impeccable/review/workers-local.png`.
- [ ] Inspect the capture once, batch-fix all material visual defects, rebuild, and capture one final confirmation.
- [ ] Verify switching back to Orchestrator still renders the original sidebar/content.
- [ ] Run `git diff --check`, inspect the final commits, and leave the worktree clean.
