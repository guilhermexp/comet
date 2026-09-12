## Context

See proposal.md for motivation and decisions D-01–D-08. The approved plan is docs/plans/2026-09-06-0238-feat-omp-chat-runtime-parity-plan.md. Native UI is GPUI; Chat Transcript is durable and Session is device execution state. Changes are serial F1/U1 → F2/U3 → F3/U2.

The OMP launcher currently awaits prompt before starting the event consumer. Local slash execution emits command_output then agentInvoked:false, without agent_end. The quick path destroys the receiver. A post-response-only drain would still deadlock if the 256-frame channel filled first.

The durable command ledger already owns deduplication, author cancellation, TTL and attachment delivery. Run is not an after-turn intent: dispatch_inner may route it as steering. Session context usage is last-known and optional. Manual compact() in the reference source does not emit automatic-compaction lifecycle events.

## Goals / Non-Goals

Goals: preserve output and terminal ordering; expose truthful compaction; deliver durable after-turn instructions without disturbing the current turn or losing Stop.

Non-goals: extension UI/Fusion/TUI, settings parity, replacing OMP command parsing/compaction, keeping child processes warm, new dependencies, Worker changes, push/deploy or restarting the current app.

## Decisions

### F1: ordered command execution

Move request/event consumption into one active driver flow so SessionStarted and output can reach consumers while prompt is outstanding. command_output becomes TextDelta and uses the existing message fold/export. Preserve event ordering through a terminal local response, and publish one Done after preceding output. Do not wait for agent_end on a local-only response. Response failure and EOF preserve partial output and report failure.

Use separate bounded deadlines for short transport requests and long local work. Do not globally disable request timeouts or add arbitrary sleeps as event fences. Normal prompts whose response omits agentInvoked retain their current behavior. Do not reconstruct local output with an LLM or duplicate it in Done.result.

### F2: Session activity, not transcript content

Normalize automatic start/end metadata and consume it at the engine before public raw-event fanout. Session carries optional bounded activity tied to the run; its existing watch feeds the UI. Only safe reason/action/outcome metadata is exposed; internal summary/context is excluded. Unknown or absent data does not manufacture a percentage or model limit.

For manual slash work observe get_state.isCompacting while its request is pending, then reconcile on response/termination. Scope the observation to the active request, one outstanding query at a time, with bounded cadence and cleanup; no idle polling. Capture actual installed-version behavior before fixing fixtures. If this observation is unavailable, stop and escalate the precise missing protocol capability rather than synthesize start from '/compact' text or edit the OMP reference checkout.

Render activity in the existing transcript trailer and context tooltip. Keep ring percentage as context occupancy, refresh only from a real usage snapshot, and wire the actual activity into idle recap. EOF, abort, error, stale run and termination clear activity. An automatic end with willRetry does not mark the turn complete.

### F3: one durable after-turn queue

Introduce FollowUp as a distinct ledger intent reusing RunRequest/message_id/attachments and Run execution at eligibility. This leaves old Run and Steer semantics intact. Do not enqueue into OMP's volatile follow_up queue and then pretend its ACK proves delivery.

Submit through QueueCommand to the actual executing host: that host must deserialize/accept FollowUp. Unsupported/offline host leaves input recoverable without fallback to Run/Steer. The existing per-entry tolerant command reader protects older observers; no container/schema rename. Reconcile ambiguous ACK by message_id before any user retry.

Before marking processed, defer FollowUp while a turn, input request, compaction or driver teardown is active. Selection must still reach eligible Interrupt/RespondInput/Steer behind it, retaining each control's own attachment gate. FollowUps remain FIFO. Re-kick drain at real settled success and relevant command/attachment transitions. Scope any scheduling change to the new intent; do not build a second scheduler.

Use engine-owned resume at dispatch, captured prompt/configuration/attachments, existing 24-hour TTL, and processed-ledger claims. Stop invalidates prior pending FollowUps by command/run identity and order, not device clock comparison. Error/crash/uncertain recovery resolves the old chain without auto-advancing; retain entries/content and a recovery reason. A claim with an uncertain side effect is not automatically replayed.

The composer keeps Enter as current send/steer and Shift+Enter as newline. Add a visible after-turn action near send; bind an additional shortcut only after confirming it is free. Project the queue from command data, not a second executor in AppState. Keep transient submission tracking separate. Echo deduplication remains message_id based; cancel is allowed only for the author's pending entry.

## Risks / Trade-offs

- Split response/event channels → preserve ordering and test output larger than channel capacity.
- New intent on an older host → reject before acceptance, preserve draft, never semantic fallback.
- Child ends between Done and next enqueue → admit only after definitive settlement; reuse a valid handle or normal respawn/resume, no warming redesign.
- Compaction errors can be recovered by OMP → distinguish maintenance outcome from terminal turn outcome.
- FollowUp behind missing bytes → controls bypass only that waiting item; no attachment validation bypass.
- Polling state for manual work costs local RPC → restrict it to the pending execution and cancel it deterministically.
- OMP source 18.0.11 differs from installed 18.1.11 → runtime capture is authoritative for fixture fields.
- GPUI has no render-test harness → artifact-level tests do not substitute native visual proof; use isolated capture profile, never the live orchestrator profile.

## Migration Plan

No deployment in this change. Preserve existing enum values, payloads and CRDT container names; new Session fields are optional. Update all exhaustive in-repo consumers and test old payload decoding. Backend must reject unsupported intent instead of silently downgrading it. Local phase commits provide rollback boundaries; no reset, destructive cleanup or rollback of unrelated work.

## Boundary Map

- F1 owns OMP driver transport/normalization and its transport tests; engine tests may prove transcript persistence without changing engine production behavior.
- F2 owns normalized activity, Session projection/watch consumers and the existing GPUI trailer/tooltip/idle recap.
- F3 owns FollowUp intent, admission/drain/attachments, submission and pending-message UI.
- Owners share files serially. Tasks/Audit state are only updated after the worker's closeout and audit. No worker may modify other phases or audit columns.
