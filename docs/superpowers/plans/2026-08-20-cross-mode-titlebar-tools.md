# Cross-mode Titlebar Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Capture and the Terminal/Git right pane available from both Orchestrator and Workers titlebars with correct per-mode context.

**Architecture:** Keep `Shell` as the shared titlebar/right-pane owner, extract pure mode-aware capability/context helpers, reuse the native Workers capture primitive, and route Orchestrator captures through the existing composer attachment API. Store right-pane state under separate Orchestrator and Workers context keys.

**Tech Stack:** Rust, GPUI, existing TerminalPanel/Changes/WorkersContent, native macOS capture menu, engine attachment RPC.

## Global Constraints

- Preserve existing titlebar dimensions, icons, hover treatment, and native menus.
- Never reuse a stale Orchestrator chat as Workers panel context.
- Orchestrator capture becomes an attachment in the active conversation.
- Workers capture remains a Worker session gallery artifact.
- Use TDD for pure routing and state seams before changing UI rendering.
- Do not commit without explicit user authorization.

---

### Task 1: Mode-aware titlebar capabilities

**Files:**
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/shell/tabs.rs`
- Test: `crates/ui/src/shell.rs`

**Interfaces:**
- Produces: a pure capability decision containing `show_capture` and `show_right_pane`.
- Consumes: sidebar mode, selected Orchestrator chat, selected Worker project/session.

- [x] Write a failing table test for Orchestrator chat/canvas and Workers selected/empty states.
- [x] Run the focused shell test and verify RED.
- [x] Implement the minimal pure capability helper.
- [x] Render both controls from the existing titlebar button primitives.
- [x] Run the focused shell test and verify GREEN.

### Task 2: Orchestrator capture attachment routing

**Files:**
- Modify: `crates/ui/src/workers/session_gallery.rs`
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/composer.rs`
- Test: `crates/ui/src/workers/session_gallery.rs`
- Test: `crates/ui/src/composer.rs`

**Interfaces:**
- Consumes: native `CaptureMode` selection and `capture_screenshot`.
- Produces: captured local file routed through the active composer's attachment API.

- [x] Write a failing test proving the shared native capture modes keep the Unpeel command contract.
- [x] Run the focused capture test and verify RED/GREEN during implementation.
- [x] Reuse the native capture request without changing Workers gallery behavior.
- [x] Add the Orchestrator capture titlebar button and route its result into the composer.
- [x] Run focused tests and verify GREEN.

### Task 3: Workers right-pane context

**Files:**
- Modify: `crates/ui/src/shell.rs`
- Modify: `crates/ui/src/shell/tabs.rs`
- Modify: `crates/ui/src/terminal/panel.rs`
- Modify: `crates/ui/src/changes.rs`
- Test: `crates/ui/src/shell.rs`

**Interfaces:**
- Produces: a Workers panel key scoped by selected project/session.
- Consumes: Worker project path, Git availability, and selected Worker session.

- [x] Write failing tests proving panel keys and stored surfaces do not cross Orchestrator/Workers modes.
- [x] Run focused shell panel tests and verify RED.
- [x] Add explicit Workers right-pane context and terminal working-directory support.
- [x] Route Git surfaces to the selected Worker project and omit Git when unavailable.
- [x] Render the shared right-pane toggle in Workers and preserve per-mode state.
- [x] Run focused tests and verify GREEN.

### Task 4: Regression and native validation

**Files:**
- Verify: `crates/ui/src/shell.rs`
- Verify: `crates/ui/src/shell/tabs.rs`
- Verify: `crates/ui/src/workers/session_gallery.rs`

**Interfaces:**
- Verifies all interfaces from Tasks 1-3 together.

- [x] Run focused UI/engine tests for shell, capture, context isolation, and explicit-cwd terminal routing.
- [x] Run formatting, `cargo check --workspace`, `cargo build -p zeron`, and `git diff --check`.
- [x] Run the Impeccable detector once over changed UI files and resolve scoped findings.
- [x] Restart dev and visually verify both action clusters and Workers Terminal/Git context.
