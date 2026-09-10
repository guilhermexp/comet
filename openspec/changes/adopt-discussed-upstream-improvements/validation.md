# Adoption validation — 2026-09-10

Source selection: discussed upstream a1adfde2. Local base b2e41d4c. P1/P3 evidence remains in their archived change. Newer fetched upstream commits are not implicitly included.

## P4 — causal recovery

- RED: `cargo test -p zeron-engine --lib chat2_host::causal_recovery_tests::parked_rows_do_not_persist_a_cursor_until_their_history_arrives -- --exact --test-threads=1` failed with persisted cursor 1 instead of 0 (`/tmp/comet-upstream-p4-red.log`).
- RED: the socket causal recovery regression failed with reconnect cursor 6 instead of 5 (`/tmp/comet-upstream-p4-transport-red.log`).
- GREEN: `cargo test -p zeron-sync --features mock-server --lib chat_client -- --test-threads=1`: 23 passed, including HTTP repair, socket repair, row-gap/backfill, quota retry and checkpoint overlap tests (`/tmp/comet-upstream-p4-sync-green.log`).
- GREEN: `cargo test -p zeron-engine --lib chat2_host -- --test-threads=1`: 3 passed; real Loro imports and SQLite snapshots prove the restored body contains `parent child`, and incomplete checkpoint import does not persist a snapshot (`/tmp/comet-upstream-p4-engine-green.log`).
- Adapted upstream 6614b324 in chat2_host, chat_client and its example/test sinks. Retained the fork's quota/backoff implementation. The one source merge conflict was formatting around the HTTP import arm; both transports now use the same causal import outcome.
- No deployed edge or live peer experiment is claimed by these in-process transport tests. Cargo commands run individually with two jobs.

## P5 — ACP lifecycle

- RED: engine became Idle instead of Working during an active prompt (`/tmp/comet-upstream-p5-engine-red.log`). Harness ended before final text (`/tmp/comet-upstream-p5-harness-red.log`), and protocol errors omitted code/data (`/tmp/comet-upstream-p5-error-red.log`). The engine fixture waits for resolved tools, independently of fork-scoped tool IDs.
- GREEN: all 9 `acp_quiet` subprocess tests passed in 38.94s, including silent tools/text/reasoning/usage, cancel, unresponsive cancel, EOF and structured errors (`/tmp/comet-upstream-p5-harness-green.log`).
- GREEN: engine `acp_lifecycle` passed in 7.20s, covering Working, subsequent steer, final turns and autonomous fallback (`/tmp/comet-upstream-p5-engine-green.log`).
- GREEN: all 156 harness unit tests passed, including progressive previews and Live Voice defaults (`/tmp/comet-upstream-p5-unit-green.log`). Full integration coverage remains in the final workspace gate.
- Adapted 32fd7070 with three-way reconciliation; preserved fork ToolCallPreview tracking, UpdateNormalizer finish_turn, RunControls, OMP and Live Voice interfaces. No live provider request was used.
