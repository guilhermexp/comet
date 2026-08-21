# Usage Widget Parity Implementation Plan

**Goal:** Complete the Comet Usage widget with the same real account quota, pace projection, and local 24h/7d/30d token/session breakdown shown by Orchestrator.dev.

**Architecture:** Keep live quota windows owned by `AgentAccounts`, extend the proto snapshot with local archive lines, and aggregate local Codex/Claude JSONL archives through bounded background readers. The GPUI sidebar derives pace and renders the same compact expanded hierarchy without hardcoded usage values.

**Tech Stack:** Rust, Tokio, Serde JSON, GPUI, Chrono.

## Global Constraints

- Use real provider/account and local archive data only.
- Honor `CODEX_HOME` and `CLAUDE_CONFIG_DIR`.
- Bound local history to 30 days and 2,000 recent files per provider.
- Preserve backward-compatible serde defaults for persisted/RPC payloads.
- Follow TDD: focused red test, minimal implementation, focused green test, then full workspace gate once.

## Task 1: Usage contract and local archive aggregation

**Files:**
- Modify: `crates/proto/src/entities.rs`
- Create: `crates/engine/src/provider_usage_archive.rs`
- Modify: `crates/engine/src/lib.rs`
- Modify: `crates/engine/src/agent_accounts.rs`

- [x] Add failing proto/engine tests for window duration and 24h/7d/30d token/session lines.
- [x] Run focused tests and confirm failure because archive lines are absent.
- [x] Add `AgentUsageLine` and `AgentAccount.usage_lines` with serde defaults; infer the two canonical window durations from normalized provider labels.
- [x] Implement bounded JSONL readers for Codex and Claude archives.
- [x] Merge local lines into the active provider account while keeping network quota windows authoritative.
- [x] Run focused tests to green.

## Task 2: Pace derivation and GPUI parity

**Files:**
- Modify: `crates/ui/src/details_sidebar/usage.rs`
- Modify: `crates/ui/src/details_sidebar/view.rs`

- [x] Add failing tests for weekly reset text, reserve/deficit, expected marker, and run-out projection.
- [x] Run focused UI tests and confirm the missing fields fail.
- [x] Extend `ProviderUsageRow` with archive lines and derived pace values.
- [x] Render the quota header, progress fill/marker, pace line, divider, and 24h/7d/30d rows using reference spacing and tones.
- [x] Run focused UI tests to green.

## Task 3: Validation

- [x] Run `cargo fmt --all -- --check`.
- [x] Run the focused engine and UI test targets.
- [x] Run `cargo test --workspace` once (all affected tests passed; one pre-existing `zeron-harness` shell fallback test failed and reproduced in isolation).
- [x] Run the Impeccable mechanical detector against the changed Usage UI.
- [x] Start the development app and visually compare the expanded Codex row with Orchestrator.dev.
