# Tasks

## 1. Controller

- [x] 1.1 `serve(reader, writer, handler)` concurrent with in-flight cancel registry; `notifications/cancelled` honoured; cancelled requests unanswered; EOF cancels pending waits. files: `crates/workers-unpeel/src/controller_mcp.rs`, `crates/workers-unpeel/src/lib.rs`. verify: `cargo test -p zeron-workers-unpeel --test controller_mcp serve`
- [x] 1.2 `wait_until` pure loop with cancel flag and `next` guidance on `timed_out`; ceiling 14400s; tool descriptions updated. files: `crates/workers-unpeel/src/controller_mcp.rs`. verify: `cargo test -p zeron-workers-unpeel --test controller_mcp wait`

## 2. Harness

- [x] 2.1 `call_timeout_for` per-call transport deadline in the OMP bridge; invariant test updated. files: `crates/harness/src/omp/workers_bridge.rs`, `crates/harness/tests/omp_rpc.rs`. verify: `cargo test -p zeron-harness workers_bridge`
- [x] 2.2 `WORKERS_CLIENT_DEADLINE_SECONDS` applied to Claude (`MCP_TOOL_TIMEOUT`) and Codex (`tool_timeout_sec`), pinned to the controller ceiling by test. files: `crates/harness/src/lib.rs`, `crates/harness/src/claude/mod.rs`, `crates/harness/src/codex/mod.rs`. verify: `cargo test -p zeron-harness native_workers_mcp`

## 3. Closeout

- [x] 3.1 fmt, clippy on touched crates, DOX pass on `crates/workers-unpeel/AGENTS.md` and `crates/harness/AGENTS.md`. verify: `cargo fmt --all -- --check && cargo clippy -p zeron-harness -p zeron-workers-unpeel --all-targets -- -D warnings`
