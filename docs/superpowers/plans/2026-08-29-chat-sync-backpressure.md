# Chat Sync Backpressure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task with review checkpoints.

**Goal:** Stop quota-rejected Chat pushes from being replayed by enqueue nudges and restore deterministic sync crate testing.

**Architecture:** Keep the existing pending queue, `quota_blocked`, retry deadline, and head retry. All send triggers consult the same quota gate; only the deadline retries the head, and ACKs resume ordered draining.

**Tech Stack:** Rust 2024, Tokio paused time, WebSocket protocol test harness, Cargo features.

**Spec:** `openspec/changes/harden-chat-sync-backpressure/`

**Global Constraints:** Do not change edge quotas/protocol, updater, app version, pinned GPUI revision, or upstream. Re-read `crates/AGENTS.md` and `crates/sync/AGENTS.md` before edits. Preserve unrelated dirty files.

## Task 1: Prove enqueue amplification

**Files:**
- Modify: `crates/sync/src/chat_client/tests.rs`
- Inspect: `crates/sync/src/chat_client.rs`

- [ ] Add `quota_rejection_blocks_enqueue_nudges_until_retry_deadline` beside the existing quota test. Arrange two pending updates, inject `code="quota"`, enqueue a third update, and assert the mock transport receives no frame before `QUOTA_RETRY`.
- [ ] Advance Tokio time to the deadline and assert exactly one `PUSH` containing the original head.
- [ ] Run `cargo test -p zeron-sync --lib quota_rejection_blocks_enqueue_nudges_until_retry_deadline -- --exact`; confirm RED because the enqueue nudge sends pending work early.

Expected assertion shape:

```rust
enqueue_update(&handle, third_update).await;
assert!(outbound.try_recv().is_err());
tokio::time::advance(QUOTA_RETRY).await;
let push = recv_push(&mut outbound).await;
assert_eq!(push.update, first_update);
assert!(outbound.try_recv().is_err());
```

## Task 2: Gate eager sends and preserve order

**Files:**
- Modify: `crates/sync/src/chat_client.rs`
- Modify: `crates/sync/src/chat_client/tests.rs`

- [ ] Add one private predicate on connection state that permits eager drain only when quota is not blocked.
- [ ] Use it in the steady-state enqueue-nudge branch before calling `push_pending`; leave the retry timer on `push_head`.
- [ ] Add `quota_retry_sends_one_head_then_ack_drains_next`: after deadline, assert one head; inject its ACK; assert only the next head is sent.
- [ ] Run both quota tests and the existing permanent/transient rejection tests.

Minimal control flow:

```rust
if !state.quota_blocked {
    push_pending(&mut socket, &state.pending).await?;
}
```

Do not clear `quota_blocked` on enqueue; clear it only through the existing successful retry/ack transition.

## Task 3: Repair the integration-test gate

**Files:**
- Modify: `crates/sync/Cargo.toml`
- Modify: `crates/sync/AGENTS.md`
- Inspect: `crates/sync/tests/registry_client.rs`

- [ ] Keep `registry::mock_server` feature-gated; do not add it to default features.
- [ ] Add an explicit integration-test target requiring `mock-server`:

```toml
[[test]]
name = "registry_client"
path = "tests/registry_client.rs"
required-features = ["mock-server"]
```

- [ ] Record `cargo test -p zeron-sync --features mock-server` as the integration gate in the local Test Coverage Matrix.
- [ ] Run the targeted registry test, then the full feature-enabled crate gate.

## Task 4: Verify and close the change

**Files:**
- Modify: `openspec/changes/harden-chat-sync-backpressure/tasks.md`

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test -p zeron-sync --features mock-server`.
- [ ] Run `cargo test` and `cargo build -p zeron`.
- [ ] Re-run `openspec validate harden-chat-sync-backpressure --strict` and mark only evidenced tasks complete.
