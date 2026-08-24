# Workers Subagent Widget Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Start Workers activities collapsed, restore live subagent status, and align Details To-dos rows with the inline renderer.

**Architecture:** Keep the existing stable-id disclosure map and lifecycle renderer. Change only their defaults/mounting, then extend the existing pure To-do layout descriptor so both visual contracts are testable without a GPUI render harness.

**Tech Stack:** Rust 2024, GPUI, `zeron-ui` co-located unit tests.

**Spec:** `docs/plans/2026-08-24-workers-subagent-widget-polish-design.md`

## Global Constraints

- Preserve avatars, transcript opening, activity ordering and chat-switch reset.
- Preserve the shared paint-local spinner and reduced-motion behavior.
- Do not change workflow/subagent transport, durable snapshots or Workers data.
- Match the inline To-do geometry exactly.

---

### Task 1: Default every activity to collapsed

**Files:**
- Modify: `crates/ui/src/details_sidebar/widgets.rs`
- Test: `crates/ui/src/details_sidebar/widgets.rs`

**Interfaces:**
- Consumes: stable activity ids supplied to `sync_activities`.
- Produces: `activity_expanded_with_default(id, false) == false` until an explicit toggle.

- [ ] Change the existing reorder test to assert both initial activities are collapsed.
- [ ] Run `cargo test -p zeron-ui workers_widget_keeps_expansion_bound_to_identity_after_reordering -- --nocapture` and verify RED.
- [ ] Remove first-item auto-expansion while retaining existing keyed values.
- [ ] Run the focused state tests and verify GREEN.

### Task 2: Restore running subagent status

**Files:**
- Modify: `crates/ui/src/details_sidebar/view.rs`

**Interfaces:**
- Consumes: `ChatActivityRow.status`.
- Produces: the existing `render_activity_status` element in every subagent header.

- [ ] Restore a stable `subagent-status-{id}` lifecycle element beside the title.
- [ ] Preserve avatar, disclosure click isolation and transcript-open behavior.
- [ ] Verify running and settled rows in the headed app because GPUI render has no automated inspection tier.

### Task 3: Match inline To-do row geometry

**Files:**
- Modify: `crates/ui/src/details_sidebar/todos.rs`
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Test: `crates/ui/src/details_sidebar/todos.rs`

**Interfaces:**
- Consumes: `todo_status_layout()`.
- Produces: row height 36, horizontal padding 12, gap 9, slot 15 and glyph 9.

- [ ] Extend the geometry test with the inline row literals and verify RED.
- [ ] Add the three row metrics to `TodoStatusLayout` and consume them in the Details widget.
- [ ] Run `cargo test -p zeron-ui todo_status_slot_centers_the_shared_glyph_geometry -- --nocapture` and verify GREEN.

### Task 4: Validate the complete slice

**Files:**
- Modify if contracts changed: `crates/ui/AGENTS.md`
- Modify: `openspec/changes/polish-workers-subagent-widget/tasks.md`

- [ ] Run `cargo test -p zeron-ui --lib`.
- [ ] Run `cargo check --workspace` once.
- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run the Impeccable detector once over the touched UI files.
- [ ] Restart the dev app and visually smoke collapsed, running, settled and To-do states.
- [ ] Review the final diff and commit locally; do not push without fresh authorization.
