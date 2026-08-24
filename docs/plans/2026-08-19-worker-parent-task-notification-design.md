# Worker Parent Task Notification Design

## Goal

When a CLI Worker launched by a primary Orchestrator session finishes, blocks,
fails, or exits, automatically resume the exact parent chat with a structured
task notification. Delivery must survive an engine or app restart and must not
repeat the same Worker event.

## Existing foundations

Comet already has the required durable delivery primitive. `SessionCommand`
entries are stored in the chat document, synchronized to the chat host, deduped
by command ID, and executed as either a steer into a live run or a resumed new
turn when no steerable run exists. `QueueWorkerNotification` adds the two
guarantees this feature needs: the parent chat must already exist, and the chat
snapshot is persisted synchronously before the RPC acknowledges delivery.

This feature uses the existing command ledger as the notification outbox. A
separate per-session hook journal records lifecycle facts only; it never queues
or executes a parent-chat turn.

## Parent identity

Every primary ACP `RunRequest` that enables the Workers MCP carries an optional
`workers_parent_chat_id`. The engine overwrites that field with the authoritative
`chat_id` before starting or resuming the harness. The ACP harness passes it only
to the controller MCP child through `COMET_WORKERS_PARENT_CHAT_ID`.

The controller consumes and removes the environment marker at startup so Worker
descendants cannot inherit controller authority or parent identity. After
`launch_worker` creates a session, it persists a binding containing:

- Worker session ID;
- parent Comet chat ID;
- registration timestamp;
- acknowledged lifecycle notification IDs;

Bindings are stored under a Comet-owned top-level key in Unpeel's guarded
`app-state.json`. This reuses its advisory lock, atomic rename, unknown-key
preservation, and shared state-change notification.

## Lifecycle source

The authoritative source for hook-capable runtimes is the Comet session host's
append-only `comet-hook-events.jsonl` inside each Worker session. Every detached
host owns a private loopback hook endpoint and injects its port into the
provider launch. Hooks therefore keep reaching the journal even when the GPUI
window is closed. Each accepted event is synchronously appended with a durable
sequence, timestamp, raw event name and runtime generation.

Generic hooks are forced into synchronous POST mode. For provider-specific
hooks that still post in a background subshell, the detached host reconciles
the atomically written `last-hook-event.json` snapshot into the journal by
filesystem identity. This recovers the newest actionable state when background
HTTP delivery is lost. A torn trailing JSONL record is repaired before the host
accepts a new event.

HTTP-only rows from asynchronous providers are held for the reconciliation
grace before becoming deliverable. That prevents an older delayed same-name
POST from being acknowledged before a newer lost snapshot has been reconciled;
the backlog contract then deterministically reports the newest actionable
state.

- `PermissionRequest` produces `waiting_for_input`;
- `Stop` or `StopFailure` produces `completed`;
- a non-live session without an actionable terminal hook produces `exited`;
- `Start`, `working` and ordinary `idle` produce no notification.

For runtimes that distrust intermediate Stops while output keeps growing, Comet
honors the upstream re-arm grace and suppresses a Stop while the session has
returned to `working`. Tagged hooks must match `runtime_generation`; untagged
legacy hooks are accepted only for the first runtime generation.

Notification identity includes `runtime_generation`, normalized event kind and
the journal sequence. Repeated questions or completed turns within one
persistent CLI generation therefore re-arm independently and cannot overwrite
one another during app downtime. If several events accumulate before delivery,
the newest actionable state replaces the older backlog in one parent turn and
the superseded sequence IDs are acknowledged atomically with it. UI-created
Workers have no binding and never notify an Orchestrator chat.

After durable parent delivery, Comet compacts the journal under a cross-process
lock to its latest latch record and bounds the acknowledgement set to that
record. The retained Start/Stop context preserves late-Stop suppression while
keeping per-refresh IO and app-state size bounded.

A Stop consumes the current Start-to-Stop episode. A later provider shutdown
hook that is also normalized to Stop is suppressed until a fresh Start is
observed, preventing the concrete Stop + SessionEnd duplicate produced by Kimi
and similar runtimes. Distinct episodes remain distinguishable until delivery.
`Exited` after an observed terminal episode is suppressed; an isolated `Exited`
without hook evidence remains actionable.

The output text is not an instruction source. At delivery time Comet reads a
bounded terminal tail, strips ANSI/control sequences, collapses whitespace, and
truncates the result. The generated prompt explicitly frames it as
Worker-reported data and points the parent at the Worker session ID for further
inspection.

## Durable delivery

For each eligible lifecycle state Comet derives stable IDs from the Worker
session, runtime generation and status:

```text
command_id = worker-notify:<session_id>:<runtime_generation>:<status>:<episode>
message_id = worker-notify-message:<session_id>:<runtime_generation>:<status>:<episode>
```

The UI-side coordinator submits a `SessionCommandPayload::Steer` through the
app-owned `QueueWorkerNotification` RPC. The RPC rejects a deleted parent,
queues the deterministic ID and persists the chat snapshot before returning.
The command ledger provides the outbox guarantees:

- a live steerable parent receives the notification immediately;
- an idle or restarted parent runs a new resumed turn;
- an offline/remote host receives the command when it reconnects;
- retrying after a crash is safe because the deterministic command ID is already
  queued or processed.

The binding acknowledges the Worker lifecycle only after the durable RPC
succeeds. A crash after command persistence but before binding acknowledgement
retries the same command ID and is therefore idempotent.

## Delivery coordinator

`WorkersModel` receives the shared `AppState` entity. On each authoritative
Workers snapshot it asks the binding store for pending lifecycle states. One in-flight set
prevents duplicate local RPC attempts while a request is unresolved. Each
successful `QueueWorkerNotification` call acknowledges its state; failures remain pending
and retry on a later refresh with exponential backoff capped at 32 seconds.
Retries do not exhaust during the app run; the deterministic command ID keeps
repeated delivery attempts idempotent.

The coordinator does not depend on the Workers sidebar being selected or a
terminal being mounted. It runs with the existing background Workers refresh.

## Error handling

- Missing parent ID: launch remains valid but no parent binding is created.
- Malformed binding state: fail closed for notification delivery and preserve
  the existing state file.
- Output read failure: deliver the lifecycle notification with `output: none`.
- Engine unavailable: do not acknowledge; retry after engine attachment.
- Parent chat deleted: the command is rejected; leave the event pending for
  capped exponential-backoff retry and log the failure without recreating a
  chat.
- Duplicate lifecycle state: deterministic command ID and acknowledged state set
  prevent an extra turn.

## Non-goals

- No new Workers UI.
- No raw terminal transcript injection.
- No polling by the Orchestrator agent.
- No notification for Workers launched manually from the sidebar.
- No replacement of existing desktop/sound notifications.
- No change to native Claude `Task`/`Agent` behavior.

## Acceptance criteria

1. A Worker launched through the controller records the exact parent chat ID.
2. A persisted Stop episode queues one automatic notification.
3. A persisted PermissionRequest queues one actionable notification.
4. Replaying an acknowledged journal sequence queues no second turn.
5. Restarting after command persistence but before binding acknowledgement is
   idempotent.
6. A manually launched Worker queues no parent notification.
7. Worker output is bounded, ANSI-free, and framed as untrusted data.
8. The existing Workers controller boundary and ACP resume tests remain green.
9. Multiple episodes written while the GPUI app is closed are preserved and
   deterministically collapsed to the newest actionable state after restart.
