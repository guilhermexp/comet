# Tasks: Fix Worker Completion Wait

## 1. OpenSpec

- [x] 1.1 Author proposal, design, spec delta, and tasks for `fix-worker-completion-wait`
- [x] 1.2 Validate `openspec validate fix-worker-completion-wait --strict`

## 2. C1 wait predicate (TDD)

- [x] 2.1 RED: `wait_for_completed_matches_live_idle_worker_with_current_episode_evidence` in `tests/controller_mcp.rs` (timeout 1800, live idle, poll cap so RED fails fast)
- [x] 2.2 RED: `current_episode_completed_*` cases in `tests/parent_notifications.rs` (idle without evidence, old generation/episode, blocked, latch after ack)
- [x] 2.3 GREEN: `current_episode_completed` reusing parent-notification evidence; `wait_until_matching` + `wait_for_status` consult it for `completed`

## 3. C2 steer consumption (TDD)

- [x] 3.1 RED: fake-omp `workers-wait-steer` leftover frame after ACK; test `steer_during_pending_host_tool_is_consumed_once_after_tool_result`
- [x] 3.2 GREEN: queue steer while host tool pending; send after `host_tool_result`; emit `Steered` on matching user `message_start`, not on ACK

## 4. C3 / docs / gates

- [x] 4.1 Negative wait cases (idle, timeout/cancel unchanged) and preserve existing omp_rpc full-run
- [x] 4.2 DOX: `crates/workers-unpeel/AGENTS.md` (own delta only) and `crates/harness/AGENTS.md`
- [ ] 4.3 Format own Rust files; `cargo test -p zeron-workers-unpeel --test controller_mcp`; `--test parent_notifications`; `cargo test -p zeron-harness --test omp_rpc`; `cargo build`; commit own deltas
