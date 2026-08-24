# Retain Context Usage Between Turns Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve the last context-window measurement while the next turn waits for an updated snapshot.

**Architecture:** Keep `Session.context_usage` as the single per-chat authority. Remove the turn-start clear and retain existing replacement/deduplication when a new `AgentEvent::Usage` arrives.

**Tech Stack:** Rust, comet-engine unit tests.

**Spec:** `docs/plans/2026-08-24-retain-context-usage-between-turns-design.md`

## Global Constraints

- Do not create UI-local duplicate usage state.
- Do not carry usage across different chat ids.
- A genuinely unmeasured chat keeps the neutral indicator.

---

### Task 1: Retain the last per-chat snapshot

**Files:**
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/engine/AGENTS.md`

**Interfaces:**
- Consumes: `Session.context_usage` and `Inner::set_context_usage`.
- Produces: last-known usage retained until the next `Some(ContextUsage)` update.

- [x] **Step 1: Write a failing last-known retention test**
- [x] **Step 2: Run the focused test and verify RED**
- [x] **Step 3: Remove only the fresh-process context clear**
- [x] **Step 4: Run focused and engine tests and verify GREEN**
- [ ] **Step 5: Commit with the companion UI fix after shared gates**
