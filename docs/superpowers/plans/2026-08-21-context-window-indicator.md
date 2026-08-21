# Context Window Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show the active chat's real context-window pressure inside the composer.

**Architecture:** Add an optional normalized `ContextUsage` snapshot to usage events and live sessions. Harnesses populate it from their native wire, the engine retains the latest trustworthy snapshot per chat, and the composer renders a neutral-or-progress ring from the selected session.

**Tech Stack:** Rust, Serde, Tokio harness streams, GPUI canvas/tooltips.

**Spec:** `docs/plans/2026-08-21-context-window-indicator-design.md`

## Global Constraints

- Never derive current context by summing historical turns.
- Old serialized events and sessions must continue to deserialize.
- Unknown context renders a neutral waiting state.
- Reuse the existing 80% warning and 95% critical thresholds.
- Do not change account quota or billing usage UI.

---

### Task 1: Add the additive protocol contract

**Files:**
- Modify: `crates/proto/src/agent.rs`
- Modify: `crates/proto/src/entities.rs`

**Interfaces:**
- Produces: `ContextUsage { tokens: u64, context_window: u64 }`.
- Extends: `AgentEvent::Usage.context_usage: Option<ContextUsage>` and `Session.context_usage: Option<ContextUsage>`.

- [ ] Add RED serde tests proving old payloads produce `None` and new payloads round-trip exact values.
- [ ] Add the additive structs/fields with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- [ ] Run `cargo test -p zeron-proto` and confirm GREEN.

### Task 2: Preserve native runtime context snapshots

**Files:**
- Modify: `crates/harness/src/codex/normalize.rs`
- Modify: `crates/harness/tests/fixtures/fake-codex.sh`
- Modify: `crates/harness/src/claude/wire.rs`
- Modify: `crates/harness/src/claude/normalize.rs`
- Modify: `crates/harness/src/claude/mod.rs`
- Modify: `crates/harness/src/omp/mod.rs`
- Modify: `crates/harness/tests/fixtures/fake-omp-rpc.sh`
- Modify: `crates/harness/src/acp/mod.rs`
- Modify: `crates/harness/src/cursor/mod.rs`

**Interfaces:**
- Codex consumes `tokenUsage.last.totalTokens` and `tokenUsage.modelContextWindow` with snake-case aliases.
- Claude consumes input/cache-read/cache-create/output tokens and the selected 200K/1M window.
- OMP consumes `get_state.contextUsage.tokens/contextWindow` immediately before `Done`.
- ACP/Cursor pass optional native context fields when present.

- [ ] Add RED normalization tests for Codex, Claude, OMP, ACP, and Cursor.
- [ ] Implement the smallest per-wire extraction without changing cumulative usage behavior.
- [ ] Run harness unit/integration suites and confirm GREEN.

### Task 3: Carry the snapshot through live sessions

**Files:**
- Modify: `crates/engine/src/sessions.rs`
- Modify: `crates/engine/tests/e2e.rs`
- Modify fixtures constructing `Session` as required by the additive Rust field.

**Interfaces:**
- Produces: session watch updates containing the last valid `context_usage` for each chat.
- Reset rule: clear when a fresh runtime process/configuration begins; retain for routed follow-ups on the same live runtime.

- [ ] Add a RED engine test covering update, retention, and fresh-runtime reset.
- [ ] Update the session reducer and workspace mirror once per changed snapshot.
- [ ] Run focused engine tests and confirm GREEN.

### Task 4: Build the composer indicator

**Files:**
- Modify: `crates/ui/src/loaders.rs`
- Modify: `crates/ui/src/composer.rs`
- Modify: `crates/ui/src/settings/accounts.rs` only if threshold helpers need crate visibility.

**Interfaces:**
- Produces: pure `ContextIndicatorState` formatting and an 18px GPUI ring.
- Consumes: `AppState::session_for(selected_chat).context_usage`.

- [ ] Add RED tests for neutral copy, percentage clamping, token formatting, and 80%/95% levels.
- [ ] Add a reusable colored ring primitive and a composer tooltip view.
- [ ] Insert the indicator immediately left of the attachment control in compact and expanded clusters.
- [ ] Run composer/UI tests and confirm GREEN.

### Task 5: Verify the real app

**Files:**
- No additional production files.

- [ ] Run format, diff, proto, harness, engine, composer, workspace-check, and Zeron build gates.
- [ ] Run the Impeccable detector once over changed UI targets.
- [ ] Launch the branch app and inspect neutral, normal, warning, critical, compact, expanded, and tooltip states in one bounded visual pass.
- [ ] Commit locally; do not merge or push without authorization.

