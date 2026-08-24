# Workers MCP Tool Cards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the real Workers MCP action and result in transcript tool cards.

**Architecture:** Normalize provider-qualified MCP calls into the existing typed `ToolCall::Mcp` model, derive the Workers action before sanitization, and retain bounded result output in the document so the existing expandable card renders it.

**Tech Stack:** Rust, ACP normalization, Zeron document projection, GPUI transcript renderer.

## Global Constraints

- Keep the unified `workers` MCP contract unchanged.
- Do not replicate large or sensitive MCP arguments into the document.
- Preserve current error styling and transcript output caps.
- Do not commit without explicit authorization.

---

### Task 1: Normalize Workers MCP calls

**Files:**
- Modify: `crates/harness/src/acp/normalize.rs`
- Test: `crates/harness/src/acp/normalize.rs`

**Interfaces:**
- Consumes: ACP `title` and `rawInput.action`.
- Produces: `ToolCall::Mcp { server, tool, input: None }` with the action as `tool`.

- [x] Add a failing test for `mcp__comet-workers__workers` plus `action=launch_worker`.
- [x] Verify the focused test fails as `ToolCall::Unknown`.
- [x] Implement provider-name parsing and Workers action promotion.
- [x] Verify the focused test passes.

### Task 2: Retain bounded result output

**Files:**
- Modify: `crates/doc/src/parts.rs`
- Test: `crates/doc/src/parts.rs`

**Interfaces:**
- Consumes: `AgentEvent::ToolResult.output` already capped by the harness.
- Produces: resolved `MessagePart::Tool.output` used by `transcript::tool_detail`.

- [x] Add a failing test proving a Workers MCP result remains visible after folding.
- [x] Verify the focused test fails because output is `None`.
- [x] Store bounded output while leaving diff behavior and argument sanitization unchanged.
- [x] Verify the focused test passes.

### Task 3: Validate presentation and regressions

**Files:**
- Verify: `crates/proto/src/view.rs`
- Verify: `crates/ui/src/transcript.rs`

**Interfaces:**
- Verifies the existing card renders invocation and result in sequence.

- [x] Run focused harness, document, and transcript tests.
- [x] Run formatting, workspace check, UI detector, and diff check.
- [x] Build the corrected dev binary; defer restart while Workers are active so their hosts survive.
