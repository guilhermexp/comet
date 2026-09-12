# Design: Fix Worker Completion Wait

## Context

`wait_for_status(status=completed)` matches only literal `activity`/`state`. After a Worker finishes the current task episode it stays `state=running` / `activity=idle` (long-lived MCP children). Parent notifications already know the episode completed (Stop or `done`+unread, quiescent output, generation/task-episode guards). Raising the wait ceiling to 4h (`2c7e645`) left that wait in flight when the Completed notification became a Steer. OMP emits `AgentEvent::Steered` on the transport ACK of `request(type=steer)` (`SteeringMode::StepBoundary` is the real contract). The engine closes the parent segment, the host tool never returns `toolResult`, and the Session stays Working.

## Decisions

- **D-01 — Reuse parent-notification evidence.** `wait_for_status(completed)` consults Stop/`done`+unread + `WorkerCompletionEvidence` + generation/task-episode. ACK stores `acknowledged_completed_generation` from the notification. The latch requires that generation plus not blocked/working and quiescent output. Missing stored generation fails closed. Idle without evidence is not completed.
- **D-02 — Injectable match seam.** `wait_until` stays host-free. A `wait_until_matching` extra predicate is how tests inject episode completion without a worker host; production `wait_for_status` passes `current_episode_completed`.
- **D-03 — Harness loop owns pending delivery and the steer queue.** Register delivering when `begin_call` is accepted. Tasks only forward the MCP outcome; the loop writes `toolResult`/fallback, then drops delivering, then drains steers if none remain. Do not use sidecar `has_pending` (it clears before delivery) and do not drain from the task.
- **D-04 — Steered is user-message lifecycle, not ACK.** Register the sent intent synchronously before dispatch. Consume on matching `message_start` (`role=user`, `steering=true`) by prompt, not `front()`. ACK does not mint a frontier and is not required before consumption. Leftover previous-task frames stay in the previous segment. Send failure removes that intent.
- **D-05 — Engine unchanged.** `sessions.rs` already treats `Steered` as a real frontier. The harness must not emit it early.
- **D-06 — Ceiling stays 4h.** Do not mask the bug by shortening the wait.

## Risks

- A steer whose ACK succeeds but whose runtime never emits the matching user `message_start` will not mint a frontier (no fabricated segment). Interrupt/shutdown still ends the run.
- `current_episode_completed` fail-softs to false on I/O errors so a wait keeps polling the literal snapshot instead of failing the tool.
