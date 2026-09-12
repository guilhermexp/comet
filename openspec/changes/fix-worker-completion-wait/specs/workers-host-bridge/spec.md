## ADDED Requirements

### Requirement: Current-episode completion satisfies wait_for_status completed

`wait_for_status` with `status=completed` SHALL return `matched: true` when the Worker’s **current** task episode has authoritative completion evidence, even if the process is still live (`state=running`) and `activity` is `idle`. Completion evidence SHALL be the existing parent-notification definition: a Stop/StopFailure hook or `done`+unread lifecycle for the current runtime generation and task episode, plus `WorkerCompletionEvidence` (output quiescent, not blocked), or the `acknowledged_completed_episode` latch for that episode. Idle without that evidence SHALL NOT match. Old generation or old task-episode completion SHALL NOT match. Human `blocked` SHALL NOT match. Timeout, cancel, and `next` guidance SHALL stay unchanged. The 4h ceiling SHALL stay unchanged.

#### Scenario: Live idle Worker with current-episode completion matches immediately

Test: `wait_for_completed_matches_live_idle_worker_with_current_episode_evidence`

- **GIVEN** a Worker with `state=running`, `activity=idle`, and current-episode completion evidence (Stop + quiescent output)
- **WHEN** `wait_for_status` is called with `status=completed` and `timeout_seconds=1800`
- **THEN** the wait returns `matched: true` without consuming the 1800s timeout

#### Scenario: Idle without completion evidence does not match

Test: `current_episode_completed_rejects_idle_without_evidence`

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

#### Scenario: Journal-less ACK is bound to the acknowledged generation

Test: `current_episode_completed_journal_less_ack_rejects_new_generation`

- **GIVEN** a Completed ACK whose stored generation is 7 and no hook journal
- **WHEN** the live snapshot reports generation 8
- **THEN** current-episode completion is false
- **AND** the same generation 7 snapshot remains completed
- **AND** a legacy ACK without a stored generation fails closed

#### Scenario: Working activity does not complete even when quiescent

Test: `current_episode_completed_rejects_working_even_when_quiescent`

- **GIVEN** Stop evidence and quiescent output while `activity=working`
- **WHEN** current-episode completion is evaluated on the journal path or the ACK latch
- **THEN** it is not completed


### Requirement: OMP steer frontier follows runtime consumption not transport ACK

While an OMP host tool call is pending delivery of its `toolResult` to the child, the harness SHALL NOT send `type=steer`. Pending lasts until the harness loop has written the result or cancel fallback, not until the sidecar MCP call returns. Two concurrent host tools SHALL keep the fence until both are delivered; finishing one SHALL NOT drain steers. `AgentEvent::Steered` SHALL be emitted only on the user `message_start` (`role=user`, `steering=true`) whose text matches a steer already registered before the RPC dispatch. Transport ACK SHALL NOT mint a frontier and SHALL NOT be required before consumption. A leftover previous-task frame SHALL stay in the previous segment. The fixture SHALL fail if `type=steer` arrives before every pending `host_tool_result`. The steer prompt SHALL be processed exactly once, including after host-tool cancel.

#### Scenario: Steer ACK during a pending host wait does not close the turn

Test: `steer_during_pending_host_tool_is_consumed_once_after_tool_result`

- **GIVEN** an OMP run with two pending `workers` host tool calls, one of which finishes first
- **WHEN** a steer arrives after a pending-tools barrier and before both `toolResult`s are written
- **THEN** the fixture sees no `type=steer` until every `host_tool_result` is delivered
- **AND** a leftover previous-task frame emitted before the steer ACK does not emit `AgentEvent::Steered`
- **AND** `message_start` for the steered user message before the transport ACK emits `AgentEvent::Steered` once
- **AND** the steer prompt is processed exactly once after that consumption

#### Scenario: Host tool cancel with a queued steer delivers cancelled result first

Test: `steer_queued_during_host_tool_cancel_is_consumed_once`

- **GIVEN** an OMP run with a held `workers` host tool call
- **WHEN** a steer is queued after the pending-tools barrier and the fixture then emits `host_tool_cancel`
- **THEN** the cancelled `host_tool_result` is delivered to OMP before `type=steer`
- **AND** a late sidecar completion does not deliver a second result or a second `AgentEvent::Steered`
- **AND** the steer prompt is processed exactly once

#### Scenario: Duplicate host_tool_call id delivers one result

Test: `duplicate_host_tool_id_delivers_one_result`

- **GIVEN** two `host_tool_call` frames with the same id
- **WHEN** the harness delivers `host_tool_result`s
- **THEN** OMP receives exactly one result for that id
- **AND** a late outcome from the original call does not send a second result
