# Orchestrator Delegation Routing Correction Design

## Goal

Make every delegation request choose one execution mechanism deterministically,
launch visible CLI Workers without racing their session host, and report progress
from authoritative lifecycle signals instead of ambiguous timestamps.

## Confirmed problems

### Ambiguous delegation surface

The primary Claude Orchestrator currently receives both:

- the native Claude `Task`/`Agent` mechanism for internal subagents; and
- the Comet-owned `mcp__comet-workers__workers` tool for persistent CLI Workers.

These are not duplicate MCP registrations, but they overlap semantically. A user
request such as “dispare um agente” can therefore start a native subagent even
when the intended result is a Worker visible in the Workers sidebar.

The observed run started a native analytical subagent, then the parent also
performed part of the same audit. After an engine restart interrupted that
subagent, the resumed Orchestrator discovered `comet-workers` and launched a
second execution path.

### Initial briefing races session-host startup

`launch_worker` receives the new session ID before the detached session host has
necessarily published its running manifest and control socket. It currently
attempts to submit `initial_text` immediately. The observed result was a Worker
created successfully but an initial briefing rejected with:

```text
Failed to connect to session host: No such file or directory (os error 2)
```

The session became usable moments later, proving a readiness race rather than an
invalid project, preset, command, or briefing.

### Non-authoritative progress timestamp

`updated_at_unix_ms` is session metadata, not a runtime heartbeat. It can remain
unchanged while the worker is producing output. Treating it as proof of activity
or inactivity creates false conclusions and unnecessary polling.

## Product contract

### User-visible delegation

- Requests to launch, dispatch, delegate to, or run a Worker default to
  `comet-workers`.
- A user-visible delegation must create exactly one persistent CLI session that
  appears in the Workers sidebar and remains inspectable after completion.
- The Orchestrator must not start a native `Task`/`Agent` for the same requested
  work before or after launching the Worker.
- The Orchestrator must not duplicate the delegated task locally while a Worker
  owns it. It may only inspect Worker state, validate its final evidence, or
  perform unrelated work.
- The Worker provider and preset are resolved from the live Comet catalog. The
  model must not invent provider IDs or raw commands when an enabled preset
  satisfies the request.

### Native analytical subagents

- Native `Task`/`Agent` remains available for internal, non-persistent analysis
  only when the user explicitly asks for an internal subagent or the task cannot
  reasonably be represented as a CLI Worker.
- Native subagents are not presented as Workers, do not satisfy requests that
  require sidebar visibility, and are not silently substituted for a Worker.
- A single delegation may use either the native mechanism or `comet-workers`,
  never both for the same scope.

### Tool discovery and naming

- `comet-workers` remains one MCP server exposing one compact `workers` tool with
  action-based dispatch. Its 13 actions must not be registered as 13 independent
  tool schemas.
- The controller MCP is injected only into primary Orchestrator ACP sessions.
  Worker CLI sessions must not inherit it and cannot recursively call
  `launch_worker`.
- Existing provider-native tools and the worker-local Unpeel MCP remain separate
  domains. They must not register a second Comet controller or an alias that
  launches the same Worker session.
- Streaming updates for one tool-call ID are revisions of one call, not separate
  invocations. Journaling, UI summaries, and telemetry must deduplicate by the
  stable tool-call ID.

## Deterministic routing rules

The Orchestrator applies the following precedence before invoking a delegation
tool:

1. An explicit Worker/provider/preset request uses `comet-workers`.
2. A request requiring a visible terminal, background persistence, later terminal
   inspection, or Workers-sidebar lifecycle uses `comet-workers`.
3. An explicit request for an internal native subagent uses `Task`/`Agent`.
4. An otherwise ambiguous “agent” delegation uses `comet-workers`, matching
   Comet's product role as an Orchestrator of CLI Workers.

The chosen route is stable for the task. An engine restart may resume the same
route, but must not switch from a native subagent to a Worker or create a second
delegation automatically. If the original route cannot be resumed, the
Orchestrator reports that fact and asks before starting a replacement.

## Reliable Worker launch

`launch_worker` remains an atomic product operation from the caller's
perspective:

1. Validate the project, preset or raw command, optional worktree, briefing size,
   and controller authority.
2. Prepare provider-specific workspace trust before spawning the runtime.
3. Create the Worker session without putting the briefing in the early launch
   payload.
4. Wait until the authoritative hosted-session manifest is `running` and the
   control socket exists, using the shared session-host readiness primitive.
5. Submit the sanitized briefing through the shared bracketed-paste, settle, and
   double-Enter pipeline.
6. Return success only after the host acknowledges the briefing delivery.

Readiness waiting is bounded. A readiness timeout returns a structured partial
launch result containing the session ID, `launched=true`,
`briefing_submitted=false`, and the readiness error. It must not claim the Worker
is executing the requested task. The created session remains available for
inspection, manual recovery, stop, or archive.

The operation must not send recovery keystrokes automatically. First-run update,
login, trust, or approval prompts are provider behavior and must be handled by an
explicit, evidence-based recovery action after inspecting the Worker.

## Progress and completion semantics

- Manifest lifecycle state identifies whether the session host is running,
  exited, or unavailable; it does not by itself prove agent work.
- Provider lifecycle/activity identifies `working`, `blocked`, or `idle` when the
  adapter provides that signal.
- Output offset advancement or a changed bounded output-tail fingerprint proves
  terminal activity when provider lifecycle is unavailable.
- `updated_at_unix_ms` is informational only and must not be described or consumed
  as a heartbeat.
- `wait_for_status` is the blocking coordination primitive. The Orchestrator must
  not replace one bounded wait with rapid manual polling.
- Completion requires the brief's explicit terminal evidence, such as a required
  signature and output artifact. `running`, `idle`, a stopped spinner, or a
  terminal prompt alone is insufficient.

## Engine restart behavior

- The existing visible “Run interrupted by engine restart” entry remains a
  truthful recovery marker for the primary Orchestrator run.
- Restart recovery must not imply that native subagents survived when their host
  process ended.
- A resumed Orchestrator may reconnect to an already-created CLI Worker by its
  stable session ID and continue monitoring it without launching a replacement.
- If a tool call completed the Worker creation but lost its response during the
  restart, recovery first lists/inspects Workers and correlates by the returned or
  persisted session ID before considering another launch.

## Data flow

```text
user delegation request
  -> deterministic route decision
     -> visible/persistent work: comet-workers
        -> validate target + prepare trust
        -> create hosted Worker session
        -> wait for running manifest + control socket
        -> submit briefing and receive acknowledgement
        -> block on lifecycle/output evidence
     -> explicit internal analysis: native Task/Agent
        -> no Workers-sidebar claims
        -> no local duplicate of the delegated scope
```

## Error handling

- Missing project or preset: fail before creating a session.
- Session host not ready before the bounded timeout: return partial launch with
  the exact session ID and no execution claim.
- Briefing delivery rejected after readiness: return partial launch and preserve
  the Worker for inspection.
- Engine restart after Worker creation: reconnect by session ID before any new
  launch.
- Native subagent lost on restart: report it as interrupted; do not silently
  replace it with a Worker.
- Duplicate tool-call frames with the same ID: fold into one invocation.
- Conflicting controller MCP registrations: fail initialization with the server
  names and sources instead of exposing two launch surfaces.

## Implementation surfaces

The eventual correction is expected to remain scoped to:

- `crates/workers-unpeel/src/controller_mcp.rs` for host readiness and structured
  partial launch results;
- `crates/workers-unpeel/tests/controller_mcp.rs` for readiness and delivery
  regressions;
- `crates/harness/src/acp/mod.rs` and its ACP tests only if runtime enforcement is
  required beyond prompt policy;
- the Orchestrator instruction/prompt source used by primary ACP sessions for the
  deterministic routing contract;
- engine/journal tests only for tool-call-ID deduplication or restart correlation
  if existing coverage does not already prove those behaviors.

This spec does not authorize removing the native Claude `Task`/`Agent` feature,
changing Worker provider presets, redesigning the Workers UI, or changing the
Unpeel MCP shipped to Worker sessions.

## Acceptance criteria

### Tool inventory

- A primary Orchestrator session exposes one `comet-workers` controller tool.
- A Worker CLI session exposes no Comet controller tool.
- Repeated streamed frames for one tool-call ID render and count as one call.

### Routing

- “Dispare um agente para revisar este projeto” launches one visible CLI Worker
  through `comet-workers` and does not invoke native `Task`/`Agent`.
- “Use um subagente interno para analisar este texto” may invoke native
  `Task`/`Agent` and creates no Worker sidebar session.
- The primary Orchestrator performs no duplicate audit commands while the
  delegated Worker owns the audit.

### Launch readiness

- A fixture whose session host publishes its socket after a delay receives the
  complete initial briefing exactly once.
- A fixture that never becomes ready returns a bounded partial-launch error with
  the created session ID and `briefing_submitted=false`.
- An already-ready host preserves the current successful launch latency within
  the readiness polling interval.

### Recovery and observation

- Restarting the primary engine after Worker creation reconnects to the same
  Worker session and does not create a duplicate.
- Frozen `updated_at_unix_ms` with advancing output is reported as active output,
  not idle.
- Completion is reported only after the brief's required signature/artifact is
  observed.

### Validation gates

- Focused controller-MCP readiness and launch tests pass.
- ACP injection and controller-boundary tests pass.
- Engine restart/deduplication tests pass when their implementation surfaces are
  changed.
- Workers and harness suites, formatting, workspace check, and native dev build
  pass.
- Native validation confirms one sidebar Worker, one briefing delivery, no
  first-call socket error, and no native subagent for the same delegation.
