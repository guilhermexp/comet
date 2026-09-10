# Adoption validation — 2026-09-10

Source selection: discussed upstream a1adfde2. Local base b2e41d4c. P1/P3 evidence remains in their archived change. Newer fetched upstream commits are not implicitly included.

## P4 — causal recovery

- RED: `cargo test -p zeron-engine --lib chat2_host::causal_recovery_tests::parked_rows_do_not_persist_a_cursor_until_their_history_arrives -- --exact --test-threads=1` failed with persisted cursor 1 instead of 0 (`/tmp/comet-upstream-p4-red.log`).
- RED: the socket causal recovery regression failed with reconnect cursor 6 instead of 5 (`/tmp/comet-upstream-p4-transport-red.log`).
- GREEN: `cargo test -p zeron-sync --features mock-server --lib chat_client -- --test-threads=1`: 23 passed, including HTTP repair, socket repair, row-gap/backfill, quota retry and checkpoint overlap tests (`/tmp/comet-upstream-p4-sync-green.log`).
- GREEN: `cargo test -p zeron-engine --lib chat2_host -- --test-threads=1`: 3 passed; real Loro imports and SQLite snapshots prove the restored body contains `parent child`, and incomplete checkpoint import does not persist a snapshot (`/tmp/comet-upstream-p4-engine-green.log`).
- Adapted upstream 6614b324 in chat2_host, chat_client and its example/test sinks. Retained the fork's quota/backoff implementation. The one source merge conflict was formatting around the HTTP import arm; both transports now use the same causal import outcome.
- No deployed edge or live peer experiment is claimed by these in-process transport tests. Cargo commands run individually with two jobs.
