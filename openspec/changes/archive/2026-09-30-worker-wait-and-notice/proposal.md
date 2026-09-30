# Change: Worker wait status contract and prompt notice delivery

## Why

On 2026-09-29 an OMP orchestrator sat idle for ~20 minutes after a Claude Worker had finished (`Stop` at 18:04:46, report on disk). Three harness defects combined:

1. `wait_for_status` accepts any `status` string. The schema says "the worker status … as reported by list_workers and inspect_worker", which never report `completed`, so the orchestrator guessed `exited`. A live Claude TUI stays `state=running`/`activity=idle` after finishing, so the wait could only time out (25 min, then 40 min). `completed` would have matched ~2 s after the Stop (`fix-worker-completion-wait`, code in `95941b2d`).
2. The `[worker-task-notification]` steer was queued at 18:04 but delivered only at 18:09:03, when the pending `wait_for_status` timed out: OMP holds steers in `queued_steers` while any host tool is in `delivering` (`crates/harness/src/omp/mod.rs`). That queueing is intentional (`fix-worker-completion-wait`: a steer during a pending host tool closed the segment without `toolResult`), but nothing ends the pending wait when the notification it is waiting for arrives.
3. The notification's output tail ended with Claude Code's composer holding a ghost prompt suggestion (`❯ status do fork do control plane?`). The orchestrator read it as a pending question and waited again. `worker_output_text`/`choose_semantic_output` keep the full viewport, with no runtime-specific chrome handling.

## What Changes

- `wait_for_status` validates `status` against a closed, documented set: `completed` (current task-episode completion) plus the lifecycle values that `list_workers`/`inspect_worker` report. An unknown value is rejected without waiting. The schema, tool description and `action=help` name `completed` as the target for "worker finished", and state that `idle` matches any pause and `exited` only a dead process.
- A wait on any status other than `completed` also returns early when the current task episode completes during the wait, marking that fact in the result.
- When a `[worker-task-notification]` is queued for a parent whose OMP turn has a pending Workers `wait_for_status`, the harness ends that wait promptly: the wait's result is delivered first, marked as interrupted by a worker notification, and then the notification prompt, exactly once. Other pending host tools keep today's queueing.
- `restart_worker` carries the parent chat binding to the replacement session, so a restarted worker still accepts tracked `send_text`, reports `completed` and notifies its parent. An unreadable binding state fails as a read error instead of "not tracked".
- Claude Workers launch with prompt suggestions disabled (`CLAUDE_CODE_ENABLE_PROMPT_SUGGESTION=false`, per the [Claude Code settings reference](https://code.claude.com/docs/en/settings-reference)), so the idle composer never carries a suggested prompt. Permission dialogs and real input requests stay visible in the output tail.

## Non-goals

- Hook events are journaled 6× per event (`comet-hook-events.jsonl`); separate defect, not addressed here.
- Native Claude/Codex orchestrators (non-OMP parents) keep today's steer handling.
- Codex composer placeholders: not observed in the incident; out of scope.

## Capabilities

### Modified Capabilities

- `workers-host-bridge`: validated/documented wait status, early return on episode completion, notification-interrupted wait, Claude Workers without prompt suggestions.

## Impact

- `crates/workers-unpeel/src/controller_mcp.rs`, `tests/controller_mcp.rs`, `AGENTS.md`.
- `crates/harness/src/omp/{mod,workers_bridge}.rs`, `tests/omp_rpc.rs` and fixtures, `AGENTS.md`.
- Claude runtime launch (`third_party/unpeel/runtimes/claude-code/` and/or `third_party/unpeel/crates/unpeel-core`), its tests and README.
- Engine/UI notification queueing (`crates/engine/src/doc_host.rs`, `crates/ui/src/workers/model.rs`) only if the harness needs a marker to recognise a worker-notification steer.
