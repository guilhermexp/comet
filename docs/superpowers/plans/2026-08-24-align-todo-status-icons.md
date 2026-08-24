# Align Todo Status Icons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Center every To-dos status glyph inside a stable circle aligned with its text row.

**Architecture:** Extract the existing status dimensions into a small pure layout descriptor used by the GPUI renderer. Apply non-shrinking flex centering to the circle; preserve all surrounding widget structure.

**Tech Stack:** Rust, GPUI, comet-ui inline unit tests.

**Spec:** `docs/plans/2026-08-24-align-todo-status-icons-design.md`

## Global Constraints

- Preserve the 32 px row height, 15 px circle, 9 px glyph, and 10 px row inset.
- Preserve current/done colors and icon semantics.
- Do not add absolute positioning or state-specific nudges.

---

### Task 1: Center the shared status slot

**Files:**
- Modify: `crates/ui/src/details_sidebar/todos.rs`
- Modify: `crates/ui/src/details_sidebar/view.rs`
- Modify: `crates/ui/AGENTS.md`

**Interfaces:**
- Consumes: `DetailsTodo::{current, done}` and existing icon constants.
- Produces: one centered `TodoStatusLayout` contract shared by all row states.

- [x] **Step 1: Write a failing geometry contract test**
- [x] **Step 2: Run the focused test and verify RED**
- [x] **Step 3: Apply the layout contract to the status circle**
- [x] **Step 4: Run focused and UI tests and verify GREEN**
- [x] **Step 5: Validate in the headed app and commit**
