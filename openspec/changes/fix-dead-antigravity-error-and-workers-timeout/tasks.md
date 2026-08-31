# Tasks

## 1. OpenSpec & TDD Red

- [x] 1.1 Create OpenSpec change `fix-dead-antigravity-error-and-workers-timeout` and validate strictly. files: `openspec/changes/fix-dead-antigravity-error-and-workers-timeout/*`. verify: `openspec validate fix-dead-antigravity-error-and-workers-timeout --strict`.
- [x] 1.2 Write TDD RED integration test in `crates/harness/tests/omp_rpc.rs` asserting transport timeout strictly exceeds controller MCP `timeout_seconds.maximum` with round-trip margin. files: `crates/harness/tests/omp_rpc.rs`, `crates/harness/Cargo.toml`. verify: `cargo test -p zeron-harness workers_bridge_timeout_strictly_exceeds_tool_blocking_ceiling`.

## 2. Implementation

- [x] 2.1 DEFEITO A: Remove dead `RefreshConfiguration` variant from `AntigravityUsageError` in `crates/engine/src/antigravity_usage.rs` and verify 0 references. files: `crates/engine/src/antigravity_usage.rs`. verify: `cargo test -p zeron-engine antigravity`.
- [x] 2.2 DEFEITO B: Elevate `TOOL_CALL_TIMEOUT` to 180s (3 minutes) in `crates/harness/src/omp/workers_bridge.rs` with single-line margin justification comment. files: `crates/harness/src/omp/workers_bridge.rs`. verify: `cargo test -p zeron-harness`.

## 3. Verification & DOX

- [x] 3.1 Verify all unit and integration tests pass across `zeron-engine`, `zeron-harness`, and `zeron-workers-unpeel`. files: workspace. verify: `cargo test -p zeron-engine antigravity && cargo test -p zeron-harness && cargo test -p zeron-workers-unpeel && cargo build -p zeron`.
- [x] 3.2 Run clippy on touched crates. files: workspace. verify: `cargo clippy -p zeron-engine -p zeron-harness -p zeron-workers-unpeel --all-targets -- -D warnings`.
- [x] 3.3 DOX pass: review and update AGENTS.md files as needed. files: `crates/AGENTS.md`, `crates/harness/AGENTS.md`. verify: local review.
