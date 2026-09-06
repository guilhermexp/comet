# Design: Fix Worker Completion Wait

## Context

`wait_for_status(status=completed)` matches only literal `activity`/`state`. After a Worker finishes the current task episode it stays `state=running` / `activity=idle` (long-lived MCP children). Parent notifications already know the episode completed (Stop or `done`+unread, quiescent output, generation/task-episode guards). Raising the wait ceiling to 4h (`2c7e645`) left that wait in flight when the Completed notification became a Steer. OMP emits `AgentEvent::Steered` on the transport ACK of `request(type=steer)` (`SteeringMode::StepBoundary` is the real contract). The engine closes the parent segment, the host tool never returns `toolResult`, and the Session stays Working.

## Decisions

- **D-01 — Reuse parent-notification evidence.** `wait_for_status(completed)` consults the same Stop/`done`+unread + `WorkerCompletionEvidence` + generation/task-episode path (and the `acknowledged_completed_episode` latch so completion stays visible after ACK). No second detector. Idle without that evidence is not completed.
- **D-02 — Injectable match seam.** `wait_until` stays host-free. A `wait_until_matching` extra predicate is how tests inject episode completion without a worker host; production `wait_for_status` passes `current_episode_completed`.
- **D-03 — Do not send steer while a host tool is in flight.** Sending `type=steer` to OMP during `wait_for_status` is what cancelled/failed the tool. Queue the `SteerMessage` until the host `toolResult` or cancel error has been written to OMP.
- **D-04 — Steered is user-message lifecycle, not ACK and not the next frame.** Installed OMP (`session.steer` → `#queueUserMessage`) ACKs when the user message is queued, not when it is consumed. Consumption is the runtime `message_start` for that user message (`role=user`, `steering=true`, text matching the sent prompt). A leftover frame from the previous task may already be queued after ACK (`agent_start`/`agent_end`/`tool_result`/isolated text do not correlate). Keep those frames in the previous segment. Do not emit on request ACK, on pending-host-tool count hitting zero, or on the first frame after ACK.
- **D-05 — Engine unchanged.** `sessions.rs` already treats `Steered` as a real frontier. The harness must not emit it early.
- **D-06 — Ceiling stays 4h.** Do not mask the bug by shortening the wait.

## Risks

- A steer whose ACK succeeds but whose runtime never emits the matching user `message_start` will not mint a frontier (no fabricated segment). Interrupt/shutdown still ends the run.
- `current_episode_completed` fail-softs to false on I/O errors so a wait keeps polling the literal snapshot instead of failing the tool.
