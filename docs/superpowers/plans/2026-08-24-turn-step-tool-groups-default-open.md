# Turn-Step Tool Groups Default-Open Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show every nested tool card by default when an expanded `TurnSteps` disclosure is visible.

**Architecture:** Reuse the existing group `auto_open` seam during completed-prefix settlement. Keep the independent per-card detail default closed and preserve explicit `FoldState` overrides.

**Tech Stack:** Rust, GPUI, inline `zeron-ui` unit tests.

**Spec:** `docs/plans/2026-08-24-turn-step-tool-groups-default-open-design.md`

## Global Constraints

- Do not change row identity, virtualization, cache, sticky-user, or scroll behavior.
- Do not auto-open command output, diffs, or invocation bodies.
- Preserve manual group collapse as an override of the default.

---

### Task 1: Default nested tool groups to visible cards

**Files:**
- Modify: `crates/ui/src/transcript.rs`
- Modify: `crates/ui/AGENTS.md`

**Interfaces:**
- Consumes: `settle_turn_steps_child(&mut Row)` and existing `FoldState.open` precedence.
- Produces: `RowKind::ToolGroup { auto_open: true, detail_auto_open: false }` for completed-prefix children.

- [x] **Step 1: Write a failing regression assertion**

Assert that the completed-prefix tool group inside `TurnSteps` has
`auto_open == true` and `detail_auto_open == false`.

- [x] **Step 2: Run the focused test and verify RED**

Run: `cargo test -p zeron-ui active_tail_tool_groups_open_while_folded_prefix_groups_settle --lib`

Expected: FAIL because settlement currently writes `auto_open = false`.

- [x] **Step 3: Implement the minimal default change**

Set only the nested group `auto_open` field to true in
`settle_turn_steps_child`; leave `detail_auto_open` false.

- [x] **Step 4: Run the renamed focused test and verify GREEN**

Run: `cargo test -p zeron-ui active_tail_and_turn_steps_tool_groups_show_cards_without_opening_details --lib`

Expected: PASS.

- [x] **Step 5: Run full gates and visual smoke**

Run the repository-required UI suite, workspace check, formatting, diff check,
Impeccable detector, build, and real-app comparison before committing.
