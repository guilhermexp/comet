# Background execution reference and current boundaries

## Kanna

- `src/client/components/chat-ui/widgets/TasksWidget.tsx:266-366` consumes Chat-scoped provider tasks with elapsed time and lifecycle.
- `src/server/background-tasks.ts:277-344` captures Claude backgrounded local Bash; foreground calls are excluded.
- `src/server/agent.ts:1227-1411` retains active tasks across turns and caps settled in-memory history.
- `src/server/local-http-servers.ts:122-169,230-316` and `PortsWidget.tsx:34-277` discover HTTP listeners separately; no general app/process inventory or OMP task integration.

## OMP source: /Users/guilhermevarela/tmp/oh-my-pi

- ExtensionContext exposes getAsyncJobSnapshot (`packages/coding-agent/src/extensibility/extensions/types.ts:455-463`); native RPC get_state has no jobs field (`modes/rpc/rpc-types.ts:99-122`).
- Bash launch result carries structured details.async job ID (`tools/bash.ts:767-795`), but it does not provide an ongoing lifecycle after the tool returns.
- Named Hub processes use launch broker snapshots (`launch/protocol.ts:49-65`) with owner, PID, start/exit and persist/detached fields. Ownership is the actual OMP session id; Comet resume identity can instead be a session-file path.
- Existing broker client can start/recover a daemon; an observational query must connect to an already-running broker and return unavailable otherwise.
- Session disposal cancels owned async jobs; persistent Hub children have a separate lifetime.

## Comet

- OMP process currently ends after terminal turn (`crates/harness/src/omp/mod.rs:1201-1209`); no idle process handle remains in engine RunHandle.
- WorkflowTask updates persist in transcript, but normal Done/boot recovery do not reconcile generic running tasks; merely accepting initial Bash job IDs would leave false active rows.
- The existing Workers process sampler uses PID+start time and session/descendant attribution (`crates/workers-unpeel/src/resources.rs:83-93,134-179`); LaunchServices apps can leave that ancestry.

## Pending product choice

The user was asked whether to integrate OMP plus a managed app launch path for actual lifecycle, or show only reported/detectable executions with unknown state for unattributed apps. Background design and dependent implementation await this choice. The independent Worker age change is implemented separately.

## Claude collection seam

`crates/harness/src/claude/normalize.rs:538-620` recognizes task lifecycle frames but deliberately excludes `local_bash`/`local_command` from the structured workflow path. Shell terminal notifications are discarded unless associated with a known agent spawn. `shell_task_notification_never_settles_a_subagent` at `:1878-1899` protects a previously observed bug where a Bash terminal notification was tagged as subagent `Done`.

Background collection must add a separate execution/task update path while retaining this invariant: a shell task notification must never settle a subagent or the orchestrator run. The current Claude path does not by itself supply the requested Background rows.
