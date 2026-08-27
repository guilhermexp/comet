# Herdr Integration — aggregate Unpeel activity in Herdr

> **Status (2026-08-11):** Aggregate adapter implemented. A TUI running in one
> Herdr pane reports one aggregate `unpeel` agent, with provider-child
> environment containment, identity-safe pane moves, unit coverage, and a
> real-PTY fake-socket case. Live multi-version/job-control smoke remains
> rollout validation. The adapter does not create panes or expose every inner
> session as a separate Herdr agent; that larger project remains deferred in
> `docs/plans/herdr-session-projection.md`.
>
> This is an optional local interoperability feature, independent of the
> critical path in `docs/plans/master-plan-next.md`. Running under Herdr
> changes neither Host ownership nor session persistence and adds no
> entitlement gate.

## Outcome

A user launches `unpeel` normally inside a Herdr pane. Herdr shows that pane
in its Agents list, and the row reflects the sessions Unpeel supervises:

```text
provider hooks + host viewport/output signals
                     |
          Unpeel ActivityEngine + rescan
                     |
            fully derived SidebarModel
                     |
          aggregate Herdr status reporter
                     |
       pane.report_agent on Herdr's local socket
                     |
             outer Unpeel Herdr pane
```

The integration activates only from a valid inherited Herdr pane context,
is silent outside Herdr, and is best-effort if the Herdr server disappears.
It sends semantic state and aggregate counts only—never terminal output,
transcripts, prompts, tool arguments, titles, paths, provider session ids,
commands, or session artifacts.

The reported agent is the **Unpeel supervisor**, not a provider process
hidden behind it. Herdr currently associates one lifecycle authority and one
agent occupant with each real pane. Separate pane-backed rows for inner
sessions are planned separately.

## Current ground truth

### Herdr

- `pane.report_agent` assigns semantic `idle`, `working`, `blocked`, or
  `unknown` state to one `pane_id`. Semantic state controls Herdr waits,
  notifications, and rollups.
- Each pane has one lifecycle authority. Reporting several inner sessions
  against the same pane would replace or race that authority, not create
  several Agents-list rows.
- Herdr derives `done` from an idle, unseen agent. A custom reporter sends
  `idle`, never `done`.
- Normal pane processes inherit `HERDR_ENV`, `HERDR_SOCKET_PATH`, and
  `HERDR_PANE_ID`. Herdr's custom-integration guide requires reports to be a
  no-op when that context is absent.
- The Socket API uses newline-delimited JSON over a Unix-domain socket on
  macOS and Linux.

Herdr 0.7.1 is the locally observed target floor, pending the two-version
smoke test in this plan. Keep the wire contract to the fields shared by that
build and the current docs: `pane_id`, `source`, `agent`, `state`, and
optional `message`, plus the matching release call. Do not send the old
`custom_status` field or require runtime schema discovery: current docs
describe `herdr api schema`, but the local 0.7.1 CLI does not provide it.

### Unpeel

- `crates/unpeel-tui/src/hook_listener.rs` receives provider hook broadcasts.
- `crates/unpeel-tui/src/activity.rs` owns the TUI's hook latch, timeout,
  durable seed, and non-hook output semantics.
- `crates/unpeel-tui/src/sessions.rs::derive_status` combines that activity
  with host liveness and `menu_prompt_active` into final
  `Starting` / `Busy` / `Idle` / `Attention` / `Exited` state.
- `App::rescan` in `crates/unpeel-tui/src/main.rs` is the point after which
  the TUI has the complete session model, whether sourced from shared files
  or the native app's `/mcp/sidebar` bridge.
- Hosted providers run in detached `unpeel-host` PTYs, not in the outer Herdr
  pane. Today their launch chain inherits the outer environment unless a
  variable is explicitly removed.

The reporter consumes the final model after rescan. Hook scripts do not post
to Herdr directly. Direct hook reporting would introduce a third activity
engine, miss hookless/output and viewport-menu states, disagree with bridge
mode, and let inner providers compete for the outer pane.

## Integration boundary

Herdr is a one-way, outbound presentation adapter for the interactive TUI.
It is not another Unpeel state bus, a Host/Controller transport, a session
truth source, or a resume mechanism. It never invokes Herdr's
`notification.show`; Herdr may still apply its own configured notification
policy to the semantic `blocked` state Unpeel reports and the `done` state
Herdr derives from `idle`.

Herdr responses can acknowledge or reject a report, but they never mutate
Unpeel state or trigger a session action. The inherited socket is never
stored under `~/.unpeel`, forwarded over `/mobile`, SSH control, Link, or
Relay, or exposed to a hosted provider.

## Aggregate contract

The outer pane owns one stable lifecycle identity:

```text
source = custom:unpeel
agent  = unpeel
pane   = inherited Herdr pane
```

### Membership and precedence

Aggregate every live, running row in the selected TUI model, including rows
that are collapsed or off-screen. Exclude stopped/exited and archived rows.
Pins and the current selection do not affect membership.

Precedence is global across that set:

| Derived Unpeel state | Herdr state | Meaning |
| --- | --- | --- |
| Any `Attention` | `blocked` | At least one session needs a user decision. |
| Otherwise any `Busy` or `Starting` | `working` | At least one session is working or launching. |
| Otherwise | `idle` | Every live session is ready, or there are no live sessions. |
| `Exited` | excluded | A stopped session is not active fleet work. |

An empty running set intentionally reports `idle` while the TUI remains
open. Consequence: Herdr may present the Unpeel pane as `done` after its last
session settles or exits, even though the TUI remains open. That is the
desired supervisor-level signal; lifecycle authority is released only when
the TUI stops owning the pane or exits.

One aggregate row necessarily collapses individual completions. If one
session settles while another remains busy, the row remains `working`.

### Messages and privacy

The optional Herdr `message` contains counts only, for example:

- `2 Unpeel sessions need attention`
- `3 working, 4 idle`
- `5 sessions idle`
- `No running sessions`

Sanitize and cap the message to Herdr's compatible display bound. Do not
include session names, commands, current prompts, paths, provider identity,
host identity, or transcript text. Do not attach `agent_session_id`: an
aggregate represents several Unpeel sessions and cannot be restored as one
native provider conversation.

## Attention and notification decision

Unpeel's final `Status::Attention` currently merges two causes:

1. a hook-owned permission/decision event such as `PermissionRequest`;
2. `menu_prompt_active`, detected from the rendered host viewport because an
   agent-drawn selection menu emits no hook.

Recommended first implementation: map every final Attention state to Herdr
`blocked`. This preserves one activity engine and matches Herdr's definition
of visible approval/question UI. It also means Herdr may apply its own
blocked/request notification policy to a viewport-only menu, while Unpeel's
own policy makes that signal badge-only with no push. Unpeel must not call
`notification.show` itself.

If the no-push rule must extend to external supervisors, add provenance
before enabling the reporter: an additive `attention_source` and pre-menu
state in `SessionRow` and the native `/mcp/sidebar` bridge. Then hook
permission maps to `blocked`, while menu-only attention reports its
underlying `working` or `idle` state with, at most, display-only metadata.
Do not re-parse hooks or manifests inside the Herdr module.

## Environment containment — Phase 0 prerequisite

The outer TUI is the only process allowed to retain its inherited Herdr pane
identity:

```text
Herdr pane
  -> unpeel TUI
     -> unpeel-host <launch-file>
        -> detached unpeel-host __session_host__
           -> provider shell / agent PTY
```

Without containment, an installed Herdr Claude, Codex, Kimi, or other
provider integration can report an inner provider's lifecycle or native
session id against the **outer Unpeel pane**. Several hosted sessions then
race each other and the aggregate reporter.

Capture Herdr context in the outer TUI, then remove every inherited variable
whose name begins with `HERDR_` at the two generic Host choke points:

1. `session_host::spawn_host_process_from_launch_file`, before it detaches
   `__session_host__`; this covers both native and TUI launchers.
2. `session_host::run_host`, from the provider `CommandBuilder` before the
   hosted PTY starts; this is defense in depth for direct host entry.

Put helpers beside `strip_leaked_launcher_env` in
`crates/unpeel-core/src/session_host.rs`, with forms for
`std::process::Command` and `portable_pty::CommandBuilder`. Prefix removal is
intentional so future Herdr/plugin variables cannot leak. A TUI-side removal
in `session_ops::spawn_session` is optional defense in depth, not a third
required contract boundary.

Preserve `UNPEEL_*`, `PATH`, provider configuration, and ordinary user
environment. This prevents accidental authority leakage; it is not a hard
same-user sandbox against a provider deliberately discovering a local
socket.

## Reporter design

### Activation and foreground ownership

Create the reporter only when all are true:

- `HERDR_ENV` is exactly `1`;
- `HERDR_SOCKET_PATH` and `HERDR_PANE_ID` are non-empty;
- one-shot CLI verb dispatch has completed and this is the interactive TUI;
- `UNPEEL_HERDR_STATUS` is not `off` (diagnostic escape hatch only).

Do not fall back to Herdr's default socket; a named Herdr session could
otherwise receive a report meant for another server. Partial/malformed pane
context and popups without a pane id are no-ops.

Before every report, heartbeat, and release, verify that this TUI still owns
its local terminal foreground process group:

```text
tcgetpgrp(STDIN_FILENO) == getpgrp()
```

Keep `source = custom:unpeel` stable. A TUI suspended with job control must
not continue reporting, and an old background TUI must not release a newer
foreground instance's authority. Reassert current state after `SIGCONT` /
foreground resume.

### Socket client

Add `crates/unpeel-tui/src/herdr.rs` as a small allowlisted protocol client.
For the current macOS/Linux TUI, send one newline-delimited JSON request at a
time to the inherited Unix socket. The allowed calls are:

- `pane.report_agent`;
- `pane.release_agent`;
- `pane.current` / `pane.list` only for safe pane-move recovery;
- optional `ping` for diagnostics.

Do not send `seq`: one worker serializes all calls, so reports cannot arrive
out of order, while restarting a sequenced reporter can leave a still-running
Herdr server rejecting lower sequence numbers.

Example report:

```json
{"id":"unpeel-17","method":"pane.report_agent","params":{"pane_id":"w1:p1","source":"custom:unpeel","agent":"unpeel","state":"working","message":"2 working, 3 idle"}}
```

Clean shutdown sends `pane.release_agent` with the same pane, source, and
agent. Unknown response fields, API errors, malformed replies, and an absent
socket are non-fatal.

### Non-blocking lifecycle

Socket work never runs on the TUI render/input thread or hook-listener
thread. A single worker owns ordering and a bounded latest-value slot:

- report immediately after the initial completed rescan;
- report when aggregate state or count message changes;
- debounce `idle` by about 250 ms to collapse hook bursts;
- replace queued state with the latest value rather than build a backlog;
- bound read/write time and response size; keep connect entirely on the
  disposable worker so a wedged local socket can never hold the TUI loop or
  extend shutdown past the lifecycle guard's bound;
- retry the latest desired state with capped exponential backoff;
- periodically reassert unchanged state so a cold Herdr restart recovers;
- distinguish last desired from last acknowledged so a failed update is not
  deduplicated forever;
- rate-limit errors into `~/.unpeel/hooks/trace.log`, never a TUI toast.

The top-level interactive TUI owns a lifecycle guard and makes a tightly
bounded release attempt on normal `q`, handled error, and unwind paths.
`SIGKILL` and severed-terminal cleanup remain Herdr's responsibility.

### Pane movement

A cross-workspace Herdr move assigns a live terminal a new public pane id,
while the process environment cannot change. Before its first report, the
reporter resolves `pane.current(caller_pane_id=HERDR_PANE_ID)` and caches the
returned stable `terminal_id`. Before every report and release it resolves
the current public id from `pane.list`, accepting only one exact terminal-id
match. This also prevents a stale-but-valid public id after a server restore
from claiming a neighboring pane.

Never omit `pane_id` and never fall back to the focused pane. If the original
terminal cannot be identified, is missing, or matches ambiguously, pause
reporting and trace the failure rather than retarget another pane.

## Implementation sequence

### Phase 0 — lock semantics and contain children

- Decide the menu-prompt notification policy.
- Confirm empty-set `idle`, stable source/agent labels, counts-only messages,
  and the locally observed Herdr 0.7.1 target floor.
- Add prefix-based `HERDR_*` removal at the two generic Host boundaries.
- Prove a real hosted child cannot observe those variables while ordinary
  and `UNPEEL_*` environment survives.

**Exit:** no inner provider or installed provider hook can claim the outer
Herdr pane.

### Phase 1 — aggregate reporter

- Add `herdr.rs` with activation, pure aggregation, request framing, response
  validation, and a latest-wins worker.
- Capture context after one-shot CLI dispatch and retain the lifecycle guard
  at the top-level interactive TUI.
- Publish only after `App::rescan`, never from the raw hook receiver.
- Add foreground gating, bounded retry/reassertion, safe pane-id recovery,
  diagnostics, and release.

**Exit:** one Unpeel row follows `blocked > working > idle` within one rescan
interval without affecting TUI responsiveness.

### Phase 2 — compatibility and rollout

- Exercise the fake-socket case alongside the existing `activity` case.
- Smoke-test the installed 0.7.1 build and the current supported Herdr
  release, including job control, pane moves, and a Herdr server restart.
- When implementation lands, document the contextual behavior and diagnostic
  opt-out in TUI user docs and add the case to the test README.

**Exit:** the integration can ship automatically with no install step, user
setting, or behavior change outside Herdr.

## Required tests and acceptance

Add a fake newline-delimited Herdr socket to
`crates/unpeel-tui/tests/harness.py` and a real-PTY
`crates/unpeel-tui/tests/cases/herdr.py` case.

Required coverage:

- exact/partial activation environment and opt-out;
- Attention > Busy/Starting > Idle precedence;
- exited/archived exclusion and accepted empty-set `idle` behavior;
- counts-only sanitization with no session content;
- initial report, deduplication, failed-update retry, and periodic reassert;
- hook `Start` / `UserPromptSubmit` → `working`;
- hook `PermissionRequest` → `blocked`;
- hook `Stop` → debounced `idle`;
- viewport-menu attention follows the recorded notification decision;
- hookless output activity affects the aggregate through the final model;
- clean `q`, handled error, and unwind release authority;
- a backgrounded TUI neither reports nor releases, and resume reasserts;
- unavailable/late/restarted socket, timeout, malformed response, and API
  error never delay or crash terminal input/rendering;
- a moved pane is recovered safely or reporting pauses—never retargets the
  focused neighbor;
- a hosted child sees no `HERDR_*` but retains expected `UNPEEL_*`;
- no inner provider source reaches the fake Herdr socket;
- smoke passes on Herdr 0.7.1 and the current supported release.

Verification:

```sh
cd crates
cargo test -p unpeel-core
cargo test -p unpeel-tui
./unpeel-tui/tests/run.sh herdr activity standalone
```

The increment is complete when `unpeel` in a normal Herdr pane appears as
exactly one aggregate agent, every report comes from the final status model,
inner sessions cannot inherit pane identity, all failure paths remain
invisible to session operation, restart/move/teardown behavior is safe, and
standalone Unpeel behavior is unchanged.

## Non-goals and guardrails

- No separate Herdr row for each Unpeel session in this plan.
- No automatic pane creation, Herdr plugin, or provider-hook modification.
- No second activity engine or internal notification channel.
- No session content or native provider session identity in reports.
- No Herdr socket forwarding or remote exposure.
- No stop/remove/archive or other session verb from Herdr.
- No diff, file tree, source editor, PR, or other IDE surface.
- No local feature gate or Link/Pro dependency.

## Related

- `docs/plans/herdr-session-projection.md` — optional one-row-per-session path
- [Herdr Socket API](https://herdr.dev/docs/socket-api/)
- [Herdr custom agent integration](https://herdr.dev/docs/integrations/#integrate-your-own-agent)
- [Herdr status authority](https://herdr.dev/docs/agents/#status-authority)
- `docs/agents/session-activity.md` — canonical activity/menu semantics
- `docs/agents/providers.md` — provider launch and hook boundaries
- `docs/plans/shared-core.md` — eventual shared owner of activity derivation
- `docs/plans/headless-host.md` — TUI-as-Host direction
- `docs/plans/host-controller-transports.md` — future selected backend model
- `docs/plans/master-plan-next.md` — canonical cross-project order
- `crates/unpeel-tui/tests/README.md` — real-binary PTY harness
