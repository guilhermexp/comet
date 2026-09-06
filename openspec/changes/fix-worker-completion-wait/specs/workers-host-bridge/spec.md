## ADDED Requirements

### Requirement: Current-episode completion satisfies wait_for_status completed

`wait_for_status` with `status=completed` SHALL return `matched: true` when the Worker’s **current** task episode has authoritative completion evidence, even if the process is still live (`state=running`) and `activity` is `idle`. Completion evidence SHALL be the existing parent-notification definition: a Stop/StopFailure hook or `done`+unread lifecycle for the current runtime generation and task episode, plus `WorkerCompletionEvidence` (output quiescent, not blocked), or the `acknowledged_completed_episode` latch for that episode. Idle without that evidence SHALL NOT match. Old generation or old task-episode completion SHALL NOT match. Human `blocked` SHALL NOT match. Timeout, cancel, and `next` guidance SHALL stay unchanged. The 4h ceiling SHALL stay unchanged.

#### Scenario: Live idle Worker with current-episode completion matches immediately

Test: `wait_for_completed_matches_live_idle_worker_with_current_episode_evidence`

- **GIVEN** a Worker with `state=running`, `activity=idle`, and current-episode completion evidence (Stop + quiescent output)
- **WHEN** `wait_for_status` is called with `status=completed` and `timeout_seconds=1800`
- **THEN** the wait returns `matched: true` without consuming the 1800s timeout

#### Scenario: Idle without completion evidence does not match

Test: `wait_for_completed_does_not_treat_idle_as_done`

- **GIVEN** a live Worker with `activity=idle` and no current-episode completion evidence
- **WHEN** `wait_for_status` is called with `status=completed`
- **THEN** the wait does not match on idle alone

#### Scenario: Old generation or old episode does not match

Test: `current_episode_completed_ignores_old_generation_and_episode`

- **GIVEN** Stop evidence for a different runtime generation or task episode
- **WHEN** current-episode completion is evaluated
- **THEN** it is not completed

#### Scenario: Blocked human input does not match completed

Test: `current_episode_completed_rejects_blocked_even_with_stop`

- **GIVEN** a Stop hook and quiescent output while `activity=blocked`
- **WHEN** current-episode completion is evaluated
- **THEN** it is not completed

### Requirement: OMP steer frontier follows runtime consumption not transport ACK

While an OMP host tool call is pending, the harness SHALL NOT send `type=steer` to the child. The host `toolResult` or cancel error SHALL be delivered first. `AgentEvent::Steered` SHALL be emitted only when the OMP runtime consumes the sent steer as a user message (`message_start` with `role=user` and `steering=true` whose text matches the sent prompt). Transport ACK of `request(type=steer)` SHALL NOT mint a frontier. The first frame after ACK SHALL NOT mint a frontier: leftover frames from the previous task may already be queued, and `agent_start`, `agent_end`, `tool_result`, or isolated later text do not correlate to the steer. Those frames SHALL stay in the previous segment. Pending host-tool count reaching zero SHALL NOT by itself mint a frontier. The steer prompt SHALL be processed exactly once. Cancel of the host tool SHALL still return a result and then allow the queued steer to be consumed.

#### Scenario: Steer ACK during a pending host wait does not close the turn

Test: `steer_during_pending_host_tool_is_consumed_once_after_tool_result`

- **GIVEN** an OMP run with a pending `workers` host tool call
- **WHEN** a steer arrives before the host tool returns
- **THEN** the host `toolResult` is delivered to OMP before `type=steer` is sent
- **AND** a leftover previous-task frame emitted after the steer ACK does not emit `AgentEvent::Steered`
- **AND** `AgentEvent::Steered` is emitted once, only on the user `message_start` that matches the sent steer
- **AND** the steer prompt is processed exactly once after that consumption
