# Worker Runtime Feedback Corrections Design

## Goal

Make delegated CLI Workers notify their exact Orchestrator parent once, and
only once, after the delegated task actually finishes. Remove Comet's remaining
dependency on Unpeel-owned hook files and correct the runtime feedback defects
observed in the `Otimizar Tamanho Bundle Craft` transcript.

## Observed failures

- Claude `Stop` is a turn boundary, not a task boundary. Monitors and background
  shells can continue after it.
- Provider `Start` currently rearms completion, so monitor wakeups generate
  repeated `completed` notifications for one delegated task.
- Both the current hook and an old `/private/tmp/.../claude-hooks.sh` are
  registered, producing duplicate lifecycle ingress.
- `read_output` exposes raw terminal repaint frames and spinner fragments.
- `updated_at_unix_ms` can remain unchanged while useful output is growing.
- `launch_worker` may return before the initial brief can be submitted through
  startup prompts.
- A Codex Stop hook can fail with exit 127 when a stale managed hook command is
  retained.

## Considered approaches

### Keep treating Stop as completed with a longer delay

Rejected. No fixed delay proves that a background build, monitor, or shell has
finished, and it still permits one notification per resumed provider turn.

### Notify only when the CLI process exits

Rejected as the sole signal. Workers are interactive persistent terminals and
normally return to a prompt without exiting.

### Delegation episode plus completion gate

Selected. The controller owns task identity, provider hooks describe runtime
facts, and completion is emitted only when the active episode settles and has
no task-owned background work. An explicit completion acknowledgement is the
strongest signal; quiescent settled state is the compatibility fallback.

## Delegation episodes

Each parent binding stores a monotonically increasing `task_episode`. The
controller increments it only when it successfully submits a new task through
`launch_worker` or `send_text(submit=true)`. Provider `Start` and monitor
wakeups do not create or rearm an episode.

Lifecycle journal rows carry the active task episode assigned by the detached
session host. A `PermissionRequest` may notify the parent during the episode,
but `Stop` merely records that the current provider turn settled.

Completion requires all of the following:

1. a Stop/StopFailure exists for the active task episode;
2. output has remained stable for the completion grace;
3. no task-owned background process or monitor remains beyond the runtime's
   baseline helper processes;
4. the episode has not already been acknowledged.

An explicit completion acknowledgement from the Worker may satisfy the output
quiescence portion, but never bypasses active task-owned background work. A CLI
exit before completion produces `exited`, not `completed`.

At most one completion notification is durable per episode. Later provider
turns and late hooks for that episode are ignored after acknowledgement.

## Process and output evidence

At episode start, capture the runtime's baseline process set. Completion checks
compare the live process tree/process group against that baseline. MCP servers
and provider helpers already present at episode start do not block completion;
new shells, monitors, builds, or child process groups do.

The session host tracks output growth time independently from the manifest's
structural update time. `updated_at_unix_ms` exposed by the controller becomes
the maximum of manifest update, output modification, and accepted hook time.

`read_output` remains offset-capable for streaming consumers, but its default
text projection uses the retained virtual terminal screen rather than raw PTY
repaint bytes. ANSI cursor movement, overwritten spinner frames, and control
characters are not returned as transcript text.

## Reliable initial submission

`launch_worker` distinguishes session creation from brief submission. It waits
for the session host, handles known first-run/update/trust prompts through the
same deterministic startup policy used by the UI, and returns success only
when the brief was submitted. If it cannot submit within the bounded startup
window, it returns a structured partial-launch error with the session ID; it
must not claim a launched task episode.

## Comet-owned hooks

Hook assets move to `~/.zeron/workers/hooks`. Worker session state may remain in
its existing compatibility location for this change, but no installed provider
configuration may point to `~/.unpeel/hooks`, `/tmp/.../claude-hooks.sh`,
`/private/tmp/.../claude-hooks.sh`, or `/var/folders/.../claude-hooks.sh`.

Runtime setup installs the Comet copy first, reconciles each provider config,
verifies that every managed command references the Comet root, and then removes
the legacy `~/.unpeel/hooks` directory. Unrelated user hooks are preserved.

The cleanup recognizes managed hook identity by filename and content marker,
not only by whether the old file still exists. This also removes the stale
Codex hook responsible for command-not-found/exit-127 failures.

## Error handling

- If process inspection fails, fail closed: retain the episode and retry.
- If output metadata cannot be read, do not declare completion from Stop alone.
- If hook migration cannot verify provider configs, keep the legacy files and
  report the migration error rather than deleting a still-referenced script.
- Parent delivery remains idempotent through the deterministic command ledger.
- Permanently deleted parents retain the existing bounded retry policy.

## Tests

- Stop while a new background child is alive produces no completion.
- The same Stop becomes one completion after child exit and quiescence.
- Multiple Start/Stop monitor turns within one episode produce one completion.
- A new controller submission creates a new episode and may complete once.
- Duplicate current/legacy Claude hooks reconcile to exactly one Comet hook.
- `/private/tmp` and `~/.unpeel/hooks` managed commands are removed while user
  hooks remain.
- Legacy hook files are deleted only after all managed provider configs verify.
- `read_output` removes spinner repaint fragments but preserves useful text.
- Output growth advances the exposed Worker update timestamp.
- Startup prompt handling either submits the brief or returns a partial-launch
  error carrying the created session ID.

## Acceptance criteria

1. The reproduced Craft episode emits no completion while its background build
   or monitor remains active.
2. It emits exactly one completion after the episode actually settles.
3. Restart/retry does not duplicate that notification.
4. No managed provider hook references or files remain under `~/.unpeel/hooks`
   after successful migration.
5. Worker output shown through MCP is readable and free of spinner repaint
   fragments.
6. Controller timestamps reflect real output activity.
7. Initial brief submission cannot silently fail behind a successful launch.

