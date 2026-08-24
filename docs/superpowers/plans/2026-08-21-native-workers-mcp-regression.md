# Native Workers MCP Regression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore `comet-workers` to native Claude Code and Codex sessions without persistent configuration changes.

**Architecture:** Reuse the existing ACP-owned canonical stdio server descriptor, then translate it into each native runtime's process-scoped configuration. Claude receives a JSON `--mcp-config`; Codex receives `-c mcp_servers.comet-workers.*` overrides when its per-session app-server starts.

**Tech Stack:** Rust, Tokio process commands, Claude Code stream-json CLI, Codex app-server JSON-RPC, MCP stdio.

**Spec:** `docs/plans/2026-08-21-native-workers-mcp-regression-design.md`

## Global Constraints

- Do not write to user or project MCP configuration files.
- Preserve native Claude/Codex drivers and their existing lifecycle behavior.
- Preserve `ZERON_DISABLE_WORKERS_MCP=1` as an opt-out.
- Do not change ACP or OMP behavior.

---

### Task 1: Expose the canonical Workers descriptor internally

**Files:**
- Modify: `crates/harness/src/acp/mod.rs`
- Test: `crates/harness/src/acp/mod.rs`

**Interfaces:**
- Produces: `pub(crate) fn workers_mcp_servers(enabled: bool, parent_chat_id: Option<&str>) -> Vec<Value>`.

- [ ] Add a failing visibility/behavior contract proving enabled requests return the current executable, controller marker, and parent chat.
- [ ] Run `cargo test -p zeron-harness acp::tests::workers_mcp --lib` and confirm RED.
- [ ] Expose the existing helper at crate scope without changing its output.
- [ ] Rerun the focused test and confirm GREEN.

### Task 2: Mount Workers in native Claude Code

**Files:**
- Modify: `crates/harness/src/claude/mod.rs`
- Test: `crates/harness/src/claude/mod.rs`

**Interfaces:**
- Produces: `claude_workers_mcp_config(&RunRequest) -> Option<String>`.
- Consumes: canonical descriptor from Task 1.

- [ ] Add failing tests proving enabled requests append `--mcp-config` with `mcpServers.comet-workers`, while disabled requests emit no MCP argument.
- [ ] Run `cargo test -p zeron-harness claude::tests::native_workers_mcp --lib` and confirm RED.
- [ ] Implement the JSON translation and append it in `ClaudeHarness::build_command`.
- [ ] Rerun Claude unit and integration tests and confirm GREEN.

### Task 3: Mount Workers in native Codex

**Files:**
- Modify: `crates/harness/src/codex/mod.rs`
- Test: `crates/harness/src/codex/mod.rs`

**Interfaces:**
- Produces: `codex_workers_mcp_overrides(&RunRequest) -> Vec<String>` and `CodexHarness::build_command`.
- Consumes: canonical descriptor from Task 1.

- [ ] Add failing tests proving enabled requests append process-scoped command, args, controller marker, and parent-chat overrides; disabled requests emit none.
- [ ] Run `cargo test -p zeron-harness codex::tests::native_workers_mcp --lib` and confirm RED.
- [ ] Extract command construction and add `-c mcp_servers.comet-workers.*` arguments.
- [ ] Rerun Codex unit and integration tests and confirm GREEN.

### Task 4: Verify end to end

**Files:**
- No production files beyond Tasks 1-3.

- [ ] Run the Workers controller MCP handshake and `tools/list`; require tool name `workers`.
- [ ] Run `cargo test -p zeron-harness --test claude`, `cargo test -p zeron-harness --test codex`, `cargo test -p zeron-harness --test omp_rpc`, the engine routing test, formatting, diff check, workspace check, and `cargo build -p zeron`.
- [ ] Launch dev from this branch and create fresh native Claude and Codex probes that verify `mcp__comet-workers__workers` is advertised.
- [ ] Commit the implementation locally; do not push or merge without authorization.

