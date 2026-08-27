<!-- Split out of the repo-root AGENTS.md. The root AGENTS.md holds the map,
hard rules, and invariants; this file is the full detail for its topic. -->

## Herdr integration

When the interactive `unpeel` TUI runs in a Herdr pane, it publishes one
aggregate Unpeel agent to that pane through Herdr's local Socket API. This is
an outbound status adapter only: Herdr does not own Unpeel sessions, and a
report never changes session state.

### Activation

The adapter starts automatically only when all three inherited values are
present and valid:

- `HERDR_ENV=1` exactly;
- a non-empty `HERDR_SOCKET_PATH`;
- a non-empty `HERDR_PANE_ID`.

It starts after one-shot CLI dispatch, so commands such as `unpeel ls` and
`unpeel send` never claim a pane. Set `UNPEEL_HERDR_STATUS=off` before
launching the TUI to disable reporting for diagnostics. Partial Herdr context
is ignored; Unpeel never guesses a default socket or focused pane.

The reporter also verifies that the TUI owns its terminal's foreground
process group before each report and release. A suspended or backgrounded TUI
therefore cannot overwrite a newer foreground authority.

### Status contract

The adapter consumes the final `SidebarModel` after `App::rescan`, not raw
provider hooks. This keeps Herdr aligned with every source already used by
the TUI: hook lifecycle, durable hook seeds, hookless output activity, Host
liveness, and rendered-viewport menu detection.

It considers every non-archived running session, including collapsed and
off-screen rows, using this precedence:

| Unpeel fleet state | Herdr state |
| --- | --- |
| Any session is `Attention` | `blocked` |
| Otherwise any session is `Busy` or `Starting` | `working` |
| Otherwise | `idle` |

Exited sessions are excluded. An open TUI with no running sessions reports
`idle`; Herdr may derive its own `done` presentation from that. Unpeel sends
only count summaries such as `2 working, 3 idle` or `No running sessions`.
It never sends session titles, prompts, commands, paths, transcripts,
provider ids, or terminal output.

Every final Attention state, including a rendered menu with no provider hook,
maps to semantic `blocked`. Unpeel never calls Herdr's notification API, but
Herdr may apply its own configured notification policy to that semantic
state.

The stable authority identity is:

```text
source = custom:unpeel
agent  = unpeel
pane   = HERDR_PANE_ID
```

Unpeel sends `pane.report_agent` and `pane.release_agent` as newline-delimited
JSON over `HERDR_SOCKET_PATH`. A background worker owns the socket, coalesces
bursts to the latest desired value, retries transient failures, and
periodically reasserts unchanged state. Socket failure never blocks terminal
input or changes Unpeel activity. On clean TUI shutdown, it makes a bounded
best-effort release while the TUI still owns the foreground. Herdr remains
responsible for cleanup if the local socket is unavailable or the process
cannot unwind.

A cross-workspace pane move can change the public pane id. The worker captures
the exact pane's stable `terminal_id` through `pane.current`; before reporting
or releasing, it accepts a public id only when `pane.list` contains exactly
one matching terminal. It never falls back to the focused pane, and ambiguity
pauses reporting instead of risking a neighboring pane.

### Child-process containment

Only the outer TUI may inherit its Herdr pane authority. Before launching a
detached session Host, and again before launching the provider PTY, Unpeel
removes every environment variable whose name begins with `HERDR_`.
`UNPEEL_*`, provider configuration, and the user's ordinary environment are
preserved.

Both boundaries are required because a provider can have its own Herdr hook.
Without containment, several hosted providers could all report against the
outer TUI pane and race the aggregate reporter. Keep this prefix removal at
the generic Host launch choke points rather than adding provider-specific
exceptions.

### Right-click inside a Herdr pane

Herdr reserves plain right-click for its own pane menu, so the TUI's context
menus are unreachable in a Herdr pane unless the user sets
`right_click_passthrough_modifier` (e.g. `"alt"`) under `[ui]` in Herdr's own
`config.toml` (available in Herdr 0.7.1 stable; no always-passthrough value
exists). This is user-side Herdr configuration — never try to work around it
in Unpeel; it is documented for users in the "Running it inside Herdr"
section of `apps/website/app/docs/terminal-ui.md`.

### Agent-list limitation

Herdr's current agent list is pane-backed: one pane has one lifecycle
authority and one agent occupant. The shipped adapter therefore produces one
aggregate Unpeel row, not one row for every hosted session. Reporting several
session ids against `HERDR_PANE_ID` would only replace or race that single
authority.

Separate session rows require explicit, real Herdr panes backed by passive
`unpeel attach` clients. That work is intentionally deferred until Unpeel has
a public cross-platform passive attach command and a frontend-neutral drive,
resize, and terminal-query lease. See
`docs/plans/herdr-session-projection.md`; do not emulate virtual rows in the
outer pane or make pane closure stop a Host session.

### Implementation and tests

- `crates/unpeel-tui/src/herdr.rs` — activation, aggregation, socket worker,
  foreground ownership, retry/reassert, and release.
- `crates/unpeel-tui/src/main.rs` — publishes the fully derived model after
  rescans and owns the reporter lifecycle.
- `crates/unpeel-core/src/session_host.rs` — strips inherited `HERDR_*` at
  both Host/provider launch boundaries.
- `crates/unpeel-tui/tests/cases/herdr.py` — real-PTY TUI lifecycle against a
  fake Herdr Unix socket.

The fake socket case covers `idle → working → blocked → idle → release` and
rejects any session content in report payloads. Focused environment tests
exercise both the detached `std::process::Command` and provider
`CommandBuilder`: every `HERDR_*` entry is removed while `UNPEEL_*`, `PATH`,
and non-prefix lookalikes survive.

Herdr integration design and compatibility decisions live in
`docs/plans/herdr-integration.md`. Herdr protocol failures are best-effort and
must remain invisible to the TUI; use the fake socket case when debugging
rather than putting network work on the render or hook-listener threads.
