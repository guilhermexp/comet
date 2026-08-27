# Herdr Session Projection — one Unpeel session per agent row

> **Status (2026-08-13): shelved — upstream-first.** Phase 0's boundary
> re-check ran against Herdr 0.8.0: the Agents list is still pane-backed
> (`pane.report_agent` requires a real pane id, one lifecycle authority per
> pane), there is still no virtual/external-agent API, and `herdr agent
> start` regressed for us — it now only launches Herdr's *known* agent kinds
> in an existing pane, no arbitrary argv (pane `split` + `run` remains the
> creation surface; `pane report-metadata` exists for sanitized titles).
>
> Product decision (2026-08-13): the pane fan-out below is **not** being
> built — too much integration surface for the value. Per-session rows wait
> on Herdr growing a virtual/external-agent API (requested upstream; see
> `docs/feature/herdr-upstream-requests.md`). If that ships, the change is a
> small extension of the existing aggregate adapter in
> `crates/unpeel-tui/src/herdr.rs` — N source-scoped reports instead of one —
> and most of this plan becomes unnecessary. The sections below are kept as
> the reference design should pane projection ever be reconsidered, and the
> public `unpeel attach` foundation remains independently desirable for
> headless/scripting work.
>
> The shipped integration stays deliberately small: the aggregate status
> adapter plus a generic one-time environment tip in the TUI (`EnvHint`,
> today used for Herdr's `right_click_passthrough_modifier`). New
> multiplexer/supervisor integrations should follow that shape — a
> self-contained env-detected adapter module — never deeper coupling.

## Outcome

An explicit user action can expose one or all live Unpeel sessions as
separate entries in Herdr's Agents list:

```text
Unpeel hosted session A <- attach client -> Herdr pane A -> agent row A
Unpeel hosted session B <- attach client -> Herdr pane B -> agent row B
Unpeel hosted session C <- attach client -> Herdr pane C -> agent row C
```

Every projected pane is a client of an existing Unpeel Host session. The
Host remains the owner of the provider PTY, manifest, output, resume state,
and lifecycle. Closing a Herdr pane detaches a viewer; it never stops,
archives, removes, restarts, or recreates the Unpeel session.

The initial projection surface is passive/read-only. Interactive control is
enabled only after a frontend-neutral single-drive lease guarantees one
writer, resizer, and terminal-query responder across the native app, phone,
TUI, and attach panes.

## Why separate rows require panes today

Herdr's current model is explicit:

- `pane.report_agent` targets one real `pane_id`;
- one lifecycle authority owns a pane;
- `agent.list` exposes those pane occupants;
- `agent.view.set` filters and sorts existing agents but does not create
  virtual entries.

Reporting several Unpeel session ids from the outer TUI pane would overwrite
or race one authority. It cannot create several rows. The available choices
are therefore:

| Shape | Agent rows | Current feasibility | Cost |
| --- | --- | --- | --- |
| Aggregate TUI | One `unpeel` supervisor | Covered by the aggregate plan | Individual completions collapse into fleet state. |
| Real projected panes | One row per projected session | Available with today's API | One attach process and pane per row. |
| Virtual/external agents | One row per session without panes | No current Herdr API | Preferred if Herdr adds it. |

This constraint is why simply adding more hook calls is not a solution.

## Product contract

### Explicit, never automatic by environment

Merely launching `unpeel` with `HERDR_ENV=1` reports the aggregate and does
nothing to the user's layout. Projection requires one of these deliberate
actions:

1. **Open selected session in Herdr** — the first shippable slice.
2. **Expose all running sessions** — a later, warned/capped batch action.

Do not auto-create panes when a session starts. A user may opt into a
reconciler later, but closing a projection suppresses recreation until they
explicitly expose it again.

### Membership

Project live/running sessions only. Archives and stopped sessions remain in
Unpeel's own archive surfaces. A projection is keyed by immutable Host id +
Unpeel session id so the same session is never opened twice accidentally.

If only some sessions are projected, remove those sessions from the outer
aggregate and keep the aggregate over the remaining live sessions. If every
live session is projected, release the aggregate authority. This prevents
double rollups without making unprojected work disappear.

### Pane identity and presentation

Each attach pane reports:

```text
agent  = unpeel
source = custom:unpeel.attach
state  = that one session's derived state
```

Use the pane id it inherits from Herdr and the same foreground/release rules
as the aggregate adapter. A sanitized Unpeel session title and, for remote
scope, a non-secret Host display label may be sent as display-only metadata
because the user explicitly opened that session. Never send prompts,
commands, transcript content, paths, provider credentials, or artifacts.

Do not label the pane `claude`, `codex`, or another inner provider. Herdr is
hosting an Unpeel attach client, and Unpeel owns provider resume/lifetime.
Do not publish a native `agent_session_id` until Herdr can restore the exact
`unpeel attach` contract rather than launching a provider directly.

## Foundation: a public single-session attach command

Ship this as part of the normal CLI:

```text
unpeel attach <session-id> [--passive | --drive]
```

Do not add a third distributed binary. Factor the useful protocol and replay
logic from:

- `apps/native/unpeel-attach/src/main.rs`
- `apps/native/unpeel-attach/src/lib.rs`
- `crates/unpeel-tui/src/stream.rs`
- `crates/unpeel-tui/src/control.rs`

The existing app helper is a strong starting point, but it is not a public
CLI contract today: it is bundled for the native surface, uses BSD/kqueue
behavior, always forwards input and resize, always advertises
`answers_queries: true`, and has no standalone detach gesture. The public
command must:

- support the shipped macOS and Linux CLI targets;
- resolve ids consistently with other `unpeel` verbs;
- replay the bounded, boundary-aligned output tail and then stream live data;
- have a local detach gesture that is never forwarded to the provider;
- restore terminal mode on every exit/error/unwind path;
- distinguish passive view from drive ownership;
- exit or enter a clearly marked review state when its Host session ends;
- remain a pure client—never mint a manifest, install hooks, or spawn a
  replacement provider.

`unpeel logs --follow` is not a substitute: it is not a faithful interactive
terminal surface and does not implement attach, input, resize, or terminal
query semantics.

## Passive projection can ship first

Multiple passive output subscribers already fit the Host model. A passive
projection must:

- subscribe to/replay output;
- advertise `answers_queries: false`;
- discard terminal stdin except its local detach gesture;
- never send `Write` or `Resize`;
- render without taking the provider PTY's grid away from its current owner.

This is sufficient to make all sessions visible and focusable in Herdr's
Agents list. It is intentionally a monitor until the user acquires drive
ownership through the later contract below.

Full-screen TUIs must be tested at a pane size different from the provider
PTY. If raw replay cannot present a stable passive view without resize, feed
the host's rendered grid through the existing VT/viewer machinery and
letterbox/crop locally. Never solve passive rendering by silently resizing
the shared provider PTY.

## Interactive control requires a drive lease

Current local control permits several clients to interfere:

- multiple `stream_input` writers can interleave bytes;
- `Resize` is last-writer-wins against one provider PTY grid;
- current native attach streams advertise `answers_queries: true`, while the
  Host only asks whether any such subscriber exists;
- the app, phone, TUI, and a Herdr attach pane may all be connected.

Add a frontend-neutral, additive per-session **drive lease**. Exactly one
holder may:

- send input;
- resize the provider PTY;
- advertise/perform terminal-query responses.

All other clients remain passive with `answers_queries: false`. Acquisition,
handoff, expiry, disconnect cleanup, and explicit takeover are Host-owned,
not Herdr-specific. A viewer never auto-steals drive merely because its pane
is focused. Older Hosts that do not advertise the capability refuse
`--drive`; clients do not guess by Host kind or a 404 probe.

The capability and operations must be additive in the shared Host descriptor
and conformance fixtures. Native app, TUI Host, phone, and future desktop/TUI
controllers use the same lease. Integrate it with existing phone/grid
handoff so there is no parallel ownership system.

## Per-pane activity source

Each attach process owns its Herdr lifecycle reporter so the row remains
correct if the outer TUI exits. It derives exactly one session's status from
the same canonical inputs used by the TUI:

- durable `last-hook-event.json` hook seed and timeout semantics;
- host manifest liveness and `menu_prompt_active`;
- output growth for hookless providers;
- remote selected-backend state when attached through a Controller.

Move or expose the single-session activity derivation for reuse; do not copy
the state machine into attach code and do not depend on the outer TUI's hook
listener remaining alive. Map `Attention → blocked`, `Busy/Starting →
working`, and `Idle → idle` under the policy chosen by the aggregate plan.

On a transient Host disconnect, report `unknown` and attempt bounded
reconnect. On a final detach, release authority. Decide before implementation
how natural provider exit behaves: the preferred Herdr-native shape is to
leave the pane in a final read-only review state long enough for `idle` to
become derived `done`, then release when the user closes it. An explicit
Unpeel remove must never leave a reconnecting zombie projection.

## Pane lifecycle and layout

The projection manager uses Herdr's supported pane-create/run surface and
records the returned pane identity in memory. It must not assume the active
or focused pane after creation.

Start with one selected session. For "Expose all running sessions":

- show the number of panes before mutation and require the explicit action;
- apply a conservative configurable-or-hard cap in the first release;
- create new panes/tabs unfocused and preserve the current layout/focus;
- deduplicate by Host id + session id;
- do not recreate a pane the user closed during this projection run;
- let existing attach panes outlive the outer TUI;
- reconcile newly started/exited sessions only while an opted-in manager is
  present, or on the next explicit expose action.

Closing the pane or using the detach gesture performs only:

1. release viewer/drive/query ownership;
2. release Herdr lifecycle authority;
3. exit the attach client.

It never sends a session lifecycle verb. Host exit may end or freeze the pane
for review, but pane exit never ends the Host.

## Remote-host behavior

Remote projection is sequenced after the Controller backend in
`docs/plans/host-controller-transports.md`. A remote attach pane is a pure
Controller client bound to immutable Host id + session id:

- use the shared HostConnection, output offsets, input, and capability path;
- keep pairing/Link/SSH credentials out of argv, environment, Herdr metadata,
  and terminal output;
- create no local manifests or hook assets;
- never launch a locally hosted `__remote_attach__` session;
- report `unknown` while offline and reconnect from the last offset;
- never silently retarget when the outer TUI changes selected Host;
- include only a sanitized Host display label to disambiguate equal titles.

The controller must not branch on Mac-app Host versus headless/TUI Host. A
drive lease is advertised as a Host capability and conformance-tested across
implementations.

## Implementation sequence

### Phase 0 — re-check the Herdr boundary

- Confirm Herdr still lacks virtual/external agent registration.
- Prefer an upstream virtual-agent surface if it now exists.
- Lock title/privacy, natural-exit review, detach gesture, layout cap, and
  aggregate-exclusion decisions.

**Exit:** the pane-backed approach is still necessary and product choices are
recorded.

### Phase 1 — passive public attach

- Factor `unpeel attach` into the distributed CLI for macOS and Linux.
- Add passive mode with no input, resize, or query-response authority.
- Reuse canonical activity derivation and the Herdr reporter for one session.
- Guarantee detach-only teardown and final review/release behavior.

**Exit:** one manually created Herdr pane can monitor one existing session,
report its status, and close without changing the Host.

### Phase 2 — open selected session

- Add the explicit TUI action using Herdr's supported pane/run surface.
- Deduplicate by Host + session and exclude that session from the aggregate.
- Preserve current focus/layout and retain attach panes after outer TUI exit.

**Exit:** a selected session appears as one separate agent row with no
duplicate aggregate contribution.

### Phase 3 — expose all running sessions

- Add a warned/capped batch action and in-memory reconciliation.
- Create only missing passive projections, unfocused.
- Respect intentional closure and release the aggregate when no sessions
  remain unprojected.

**Exit:** `N` live sessions produce `N` Herdr rows without pane churn,
duplicates, or Host lifecycle changes.

### Phase 4 — optional interactive drive

- Add the advertised generic drive/query/grid lease to the shared Host
  contract and both Host implementations.
- Port app, phone, TUI, and attach surfaces to it.
- Enable explicit `--drive` acquisition/takeover only after conformance and
  multi-client stress tests pass.

**Exit:** exactly one connected client can write, resize, and answer terminal
queries, with deterministic handoff and no Host-kind branching.

### Phase 5 — remote projections

- Reuse the completed TUI Controller backend and HostConnection.
- Add immutable remote binding, offline/unknown, offset reconnect, and secure
  credential lookup.

**Exit:** the same attach pane works against any advertised Host without
creating local session state.

## Required tests and acceptance

### Passive/local

- `unpeel attach <id> --passive` replays and follows output on macOS/Linux;
- passive mode sends no input, resize, or terminal-query replies;
- terminal mode restores on detach, error, host exit, and unwind;
- pane close/detach leaves the Host pid, manifest, and control socket alive;
- one projected session yields one Herdr row and is removed from aggregate;
- `N` live sessions yield `N` rows with no aggregate duplicate;
- outer TUI exit leaves attach panes and their reporters alive;
- host natural exit follows the decided done/review behavior;
- remove/archive cannot create a reconnecting projection;
- duplicate/open-all actions do not duplicate panes;
- explicit pane closure is not immediately undone;
- focus and existing Herdr layout remain stable; high counts hit the cap;
- titles are sanitized and no prohibited session content reaches Herdr.

### Drive lease

- two viewers can receive output concurrently;
- only the lease holder can write or resize;
- only the lease holder advertises `answers_queries: true`;
- takeover/handoff is atomic and a disconnected holder is cleaned up;
- app, phone, TUI, and Herdr clients cannot interleave input or oscillate
  grid size under stress;
- an older Host refuses drive without a capability guess;
- native and TUI Hosts pass the same conformance cases.

### Remote

- the pane stays bound to captured Host id + session id across scope changes;
- offline reports `unknown`, then resumes from its output offset;
- credentials never appear in argv/env/metadata/logs;
- no local manifest, hook install, or hosted attach session is created;
- Mac-app and headless Hosts behave identically through advertised
  capabilities.

This plan is complete when every explicitly projected live session has one
accurate Herdr row backed by a real detach-only pane, passive fan-out cannot
affect a Host PTY, interactive mode has exactly one drive/query/grid owner,
and remote mode remains a pure Controller client.

## Non-goals and guardrails

- No virtual-row emulation against one outer pane.
- No automatic mass pane creation from environment detection.
- No archive/stopped-session browser in Herdr.
- No pane-close-to-stop behavior.
- No direct provider launch or native provider resume claim from Herdr.
- No duplicate activity engine or provider-hook modifications.
- No credentials or session content beyond an explicitly visible sanitized
  title/Host label.
- No Host-kind branching or capability-by-404 probing.
- No diff, file tree, source editor, PR, or other IDE surface.
- No Link/Pro gate on local or direct behavior.

## Related

- `docs/plans/herdr-integration.md` — aggregate reporter and Herdr lifecycle
  adapter
- [Herdr Socket API](https://herdr.dev/docs/socket-api/)
- [Herdr status authority](https://herdr.dev/docs/agents/#status-authority)
- `apps/native/unpeel-attach` — current native attach client
- `docs/agents/terminal.md` — attach, grid, phone, and query-response rules
- `docs/agents/session-activity.md` — canonical status derivation
- `docs/plans/host-controller-transports.md` — remote Controller sequencing
- `protocol/host-capabilities-v1.json` — advertised Host operations
- `protocol/host-conformance-v1.json` — cross-Host behavior cases
