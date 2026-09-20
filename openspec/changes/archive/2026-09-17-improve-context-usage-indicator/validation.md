# Validation — 2026-09-17

Compared/ported behavior from upstream 8ee7a622, preserving fork Session persistence and OMP compaction. No changes to the unrelated controller_mcp.rs edit; the pre-existing OMP delegation-policy edit remains intact.

## Automated
- `cargo test -p zeron-proto -p zeron-doc -p zeron-harness -p zeron-engine -p zeron-ui --lib context`: 49 passed. Includes a GPUI entity-observer regression that changes usage while the same tooltip remains alive and verifies notification/current values.
- `cargo test -p zeron-proto -p zeron-doc -p zeron-harness --lib`: 374 passed.
- `cargo test -p zeron-harness --test claude --test codex --test omp_rpc`: 87 passed, 6 existing real-runtime tests ignored. OMP background ACK, increased counts, missing usage, failure, timeout and cancellation covered.
- `cargo test -p zeron-engine --test e2e context_usage -- --test-threads=1`: 2 passed (turn continuity and actual engine restart).
- `cargo build -p zeron --bin zeron`: passed.
- `cargo fmt --all`, `git diff --check`, OpenSpec strict validation: passed.

## Native UI
Built a separate app bundle/profile at /tmp/comet-context-usage-qa, IPC 27937, with a local copy of the fake OMP RPC fixture. No paid model calls. Fixture model/thinking gates were relaxed solely in /tmp to accept the picker-selected model. An initial strict-fixture model rejection was resolved before the successful check; repository fixture remains unchanged apart from the added ContextUsage field in Rust tests.

Observed in the selected OMP Chat:
1. Initial reported 16000 / 828000: circle + 2%, tooltip 2% used / 98% left and 812000 tokens remaining.
2. Click circle: /compact submitted, indicator switches to Compacting while terminal completion is withheld.
3. Release fixture: marker Context compacted · 16k → 59k and circle 7% appear without another turn.
4. Tooltip shows 59000 / 828000 tokens and 769000 remaining.
5. Unknown usage in the other Chat shows an em dash beside the circle.

The tooltip keeps fork glass styling. Continuous per-token OMP telemetry is not added: the runtime is queried at turn/command completion. The production application was not restarted; the updated binary is ready on disk.
