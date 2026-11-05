# Tasks

## 1. Capture the resume difference

- [x] 1.1 Capture the full provider request body of the last request of an OMP process and the first request of the next process resumed through the current adapter sequence (spawn → `switch_session` → `set_host_tools` → `set_model` → `set_thinking_level` → `prompt`). Run the same capture for `omp -p -c` as control, with no OMP source change. Verify by saving both diffs and a short note naming the differing fields under `.tmp/verify/preserve-omp-prompt-cache-across-turns/request-diff/`.

## 2. Warm runtime between turns

- [x] 2.1 Make `run_session` keep the process, bridges and loop alive after a `Complete` disposition, including compaction completion, and track whether a turn is active. Verify with a new `tests/omp_rpc.rs` fixture test where two turns produce one launch, one `set_host_tools` and exactly one `Done` per turn.
- [x] 2.2 Send a message that arrives with no active turn as a new `prompt` (same attachment/image preflight and `prompt_timeout` as the initial prompt), and keep `steer` for messages during a running turn. Verify with fixture tests for idle message → new turn, and running-turn message → steer consumed once, with `run_streams_resumes_steers_answers_and_completes_once` and the existing steer and host-tool tests still green.
- [x] 2.3 End the loop and shut down the bridges and process on interrupt, closed steering mailbox while idle, transport or prompt failure, and event receiver close. Verify with fixture tests that each path leaves no child process, and that a pending `wait_for_status` or interactive request across a turn boundary resolves exactly once.
- [x] 2.4 Confirm that the engine parks the warm OMP runtime and routes the next dispatch into it, that the reaper, Stop, model/reasoning change and update-pending retire it, and that an autonomous wake after `Done` is handled without a duplicate `Done`. Adjust `sessions.rs` only where a test shows a gap. Verify with engine integration tests that use the controlled harness (`cargo test -p zeron-engine --test <file> <filter>`, named in the delivery).
- [x] 2.5 Update `crates/harness/AGENTS.md` (OMP lifecycle bullet and Test Coverage Matrix rows) and fix the OMP parking claim in `crates/engine/AGENTS.md`. Verify that the docs name the new tests and match the observed behavior.

## 3. Resume parity

- [x] 3.1 Change the adapter resume sequence or launch arguments, using the diff from 1.1, so a resumed process sends the same system prompt and no tool-roster notice for host tools the conversation already had. Verify with fixture tests that pin the new RPC order or launch argv, and with a re-run of the 1.1 capture showing no prefix difference.
- [x] 3.2 Add an opt-in live test with the real `omp` binary, gated by an env var and skipped by default. It runs at least 4 turns with a process restart between turns, host tools registered and thinking enabled, and asserts the spec's live scenario: digest unchanged, no roster notice, no `thinking_dropped`, and cache read ≥ previous prompt − 5,000 tokens. Verify by running it once and saving its output under `.tmp/verify/preserve-omp-prompt-cache-across-turns/live/`.

## 4. Integration

- [x] 4.1 Run `cargo fmt --all --check`, `cargo test -p zeron-harness --test omp_rpc`, `cargo test -p zeron-harness --lib omp` and the engine tests from 2.4, using a task-exclusive `CARGO_TARGET_DIR`. Verify that all pass, with their output in the delivery.
- [x] 4.2 Run the pre-commit gate `/Users/guilhermevarela/orchestrator.v5/scripts/review-pair.sh --repo <checkout> --base <sha> --out .tmp/verify/preserve-omp-prompt-cache-across-turns/review-pair/<n> --include-untracked --gate`, or the manual-review contingency under `.tmp/verify/preserve-omp-prompt-cache-across-turns/manual-review/<n>/summary.md` on exit 4. Verify with exit 0 or a complete manual summary with no pending findings.

## Workflow follow-up

- After release, the orchestrator repeats the 5-turn Opus test through the installed Comet: digest constant, zero `thinking_dropped`, and every turn-start cache read ≥ previous prompt − 5,000 tokens.
- Archive the change, `/sync` the project AGENTS.md, `/cleanup` the branch, worktree and task build directories.
