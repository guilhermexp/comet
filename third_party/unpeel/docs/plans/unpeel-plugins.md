# Unpeel Apps implementation — CLI packages rendered by every frontend

> **Terminology (2026-08-10):** the product term is **Unpeel App**, never
> plugin. This historical filename remains the detailed implementation plan
> for `unpeel-ui`, view targets, sidebar presence, preset injection, and the
> long-term semantic protocol. `docs/plans/unpeel-apps.md` is authoritative for
> the app contract, RoomStore, and how every app may use Unpeel Link. Older
> body text and placeholder names still say “plugin”; read those as “App” until
> the implementation terminology is migrated. New product copy and APIs use
> App.

> **Public package naming (decided 2026-08-15):** the pre-publication
> `unpeel-ui` crate becomes the **Unpeel Apps UI SDK**, package
> `unpeel-apps-ui-sdk`, before public SDK release. Historical implementation
> references below retain `unpeel-ui` where they describe today's filesystem
> and crate; the semantic wire name remains `unpeel.ui/1`.

> **Status (2026-08-11):** Horizon A started. `crates/unpeel-ui` exists —
> the A1 extraction (style/spinner/shimmer/hints, fuzzy scoring, nav-key
> conventions) plus `host::Host::detect()` and `status::StatusReporter`
> (canonical hook events + durable seed, `status.json` marker + bus
> announce; every call a silent no-op standalone). The proving plugin is
> `crates/unpeel-apps/unpeel-todos` (filling A2's `unpeel-tasks` role): a complete
> standalone todo TUI (`unpeel-todos`, plain-file store at
> `~/.unpeel/todos.json`, `--file` override). Not built yet: sidebar
> rendering of `status.json` in either frontend, preset auto-injection
> (A5), panels, and the widget rail (target decided 2026-08-12). The first Horizon B **package slice now exists**:
> `unpeel-ui` has portable Ratatui-shaped owned specs for layout, paragraph,
> tabs, lists, tables, and recording canvas; stable generic reorder behavior
> for layout children/cards, tabs, list items, table rows, and table columns;
> `unpeel.ui/1` framing/actions/schema/fixtures; validation; a real-Ratatui
> adapter; and a dual-mode example. Broad built-in coverage remains. Host
> structured-session transport, frontend integration, and SwiftUI/web
> renderers are not built.
> Decision: Horizon A remains the shipped universal path; Horizon B is an
> optional native interpretation of the same App, with raw Ratatui/PTY
> fallback forever. Horizon B shares its eventual hosted-session machinery
> with `docs/plans/dual-mode-sessions.md`; library/schema work may proceed
> independently, but integration cannot ship before that plan's Phase 1.

## The idea

A **plugin** is a CLI package (`unpeel-tasks`, `unpeel-notes`, …) that adds a
non-agent surface to Unpeel — launched, persisted, streamed, and remoted
exactly like any session. Unpeel's thesis already implies this: the terminal
is a universal render target, and Unpeel makes terminal programs pleasant on
desktop, in the TUI, and on the phone. Plugins are that thesis applied to
first-party (and eventually third-party) tools, for any domain — tasks,
notes, dashboards, ops — never code-editor chrome (Product Philosophy in
AGENTS.md applies to plugins with full force).

Two horizons, one product shape:

- **Horizon A — plugins are TUI programs.** A plugin is a Ratatui binary. It
  renders everywhere *today* through the existing hosted-PTY stack: Ghostty
  surface on Mac, the TUI attaches like any session, the phone streams it
  over the shipped remote protocol. Zero new rendering code in any frontend.
- **Horizon B — Apps emit portable Ratatui-shaped specs, frontends interpret
  them.** The same App process speaks a versioned declarative protocol over
  stdio; the terminal adapter renders real Ratatui widgets, Mac and iPhone use
  SwiftUI, and a future web client can use DOM/SVG/canvas. Native clients keep
  widget semantics, touch targets, and accessibility without copying terminal
  cells. Raw/custom Ratatui rendering remains the universal fallback forever.

## Three view targets: surface, panel, and widget

Orthogonal to the horizons: a plugin renders into one of three **targets**.
Same package, same protocol — the target changes where it appears, not what
it is. Targets multiply *placements*, never packages: there are no "full
apps" vs "widget apps" as separate kinds — one App can render as a surface
when opened, a panel when an agent surfaces it, and a widget in the rail.
Typing the apps would fork the ecosystem; targeting the views does not.

- **Surface** — the full session area. Everything above assumes this.
- **Panel** — a column beside a session (a terminal today, a dual-mode chat
  session later): the artifacts/companion column. Claude.ai's artifact pane
  is the reference feel; Unpeel's version stays inside the review-surface
  rule — screenshots, rendered docs, task lists, dashboards. The panel is
  where "never IDE chrome" earns its keep: a column next to an agent will
  tempt diffs and file trees, and the vocabulary must keep refusing them.
- **Widget** (decided 2026-08-12) — a compact, always-on tile in the
  optional **widget rail**: a persistent user-composed column of small App
  views, stacked with resizable panes. Detail below.

A native App view stacked above **its own** live terminal is a presentation of
the App's surface, not a third target and not a panel. It requires a dedicated
Host UI side channel because the PTY must remain free of JSON frames. The
focused architecture is `docs/plans/unpeel-app-native-rendering.md`.

Two kinds of panel content, deliberately different in cost:

1. **Built-in artifact viewer — no plugin, no process.** Renders files from
   the session's existing `~/.unpeel/app-sessions/<id>/artifacts/` dir (the
   browser screenshot the agent just took, a markdown doc). This is
   "screenshots are the review surface" made resident instead of
   click-to-open. Pure frontend work in each UI; nothing new in the host.
2. **Plugin panel.** Horizon A: the panel is a **companion hosted session**
   — another PTY rendered narrow (a second Ghostty column on desktop, a
   split in the TUI). Horizon B: a semantic-tree plugin rendered natively,
   which is where panels really shine — narrow character grids are cramped,
   native lists are not.

The session↔panel pairing lives in shared state (a `panel.json` marker or a
manifest field, announced on the state bus) so both frontends show the same
pairing; column sizing, splits, and dismissal are per-UI presentation.

**Mouse in the TUI is not a blocker.** The TUI already captures full mouse
input (clicks, drag, wheel, motion) and already forwards the wheel into
focused panes as SGR sequences at pane-relative coordinates — the tmux
model. Panel work extends that: draggable splitters (cell-granular,
persisted in `tui-layout.json`), and click/drag forwarding into a plugin
pane when the inner app has mouse reporting on (libghostty-vt tracks the
mode). All escape sequences, so it works unchanged over SSH — which the
headless host relies on. Cell resolution and the capture-vs-native-selection
trade (the TUI's selection mode already releases capture) are accepted
terminal realities, and part of why Horizon B renders panels natively.

### Widgets and the rail

The widget rail is an optional, always-on column of small App views — off by
default, composed entirely by the user. Like panels, widget content comes in
two tiers, deliberately different in cost:

1. **Status-row widget — no process.** The A3 machinery already is a
   widget-lite: activity plus the `status.json` line ("3 open · 1 done"). The
   rail's default tier is a richer rendering of that same marker — a tile
   showing the App's name, status text, and activity state — still zero
   processes. A glanceable number must never cost a running session.
2. **Live-pane widget.** Horizon A: a pinned **companion hosted session** — a
   real PTY rendered small in the rail's stack. This is literally "multiple
   terminals with resizable panes", and it reuses the panel machinery
   wholesale: cell-granular draggable splitters persisted in
   `tui-layout.json`, mouse forwarding, a Ghostty column on desktop, splits
   in the TUI. Always-on means always-running — each live widget is a hosted
   session with the full manifest/heartbeat/reap discipline, so the rail's
   process cost is visible and honest, never a hidden daemon (the
   no-app-specific-daemon guardrail applies). Horizon B is where widgets
   truly shine: a tiny character grid is even more cramped than a panel
   column, while a semantic tree renders a proper compact native tile.

Expanding a status-row widget **promotes** it to a live pane (spawning the
session, a user action like any launch); collapsing demotes it back to the
marker tier and stops the session. The two tiers are one widget at two
zoom levels, not two features.

**Scope is per widget entry, not per rail** (decided 2026-08-12). Each rail
entry is `global` or `project(id)`; there is one rail, which renders the
global entries plus the entries scoped to the **active project**. This is
the pins/presets shape — one flat user-owned list filtered by context —
never N independent rails. Project scope is the natural default (a todos or
dashboard widget is about *this* project, and a Horizon A live widget
already binds to a cwd — the session *is* project-scoped; the entry's scope
only governs visibility), while global entries cover the genuinely
cross-project tiles (attention-across-agents, a personal list) without
duplicating them into every project.

The active project is not a new concept: it is the already-shipped
derivation the app surfaces above the terminal — the selected session's
project, falling back to the first project (`activeProjectID` in
`RootView.swift`; the TUI derives the same from its focused session). The
rail is therefore contextual: focusing a session in another project swaps
the project-scoped tiles. That swap must be flash-free (perceived-speed
rule): status-row tiles are cheap; a live pane entering/leaving needs stable
layout — reserved slot or animated collapse, never a reflow jump.

**Visibility is not lifecycle.** A project-scoped live widget whose project
loses focus *hides*; its hosted session keeps running like any other, and
stopping it remains the explicit demote action. Hiding must never kill — a
rail that reaps sessions on focus change would feel destructive. The honest
corollary: live widgets across many projects means that many idle sessions,
which is exactly why the zero-process status-row tier is the default.

The rail also scopes with the **Host picker**: when the UI is scoped to a
remote host, the rail shows that Host's widgets — its sessions, its
`status.json` markers — through the same remote protocol and purity rule
(never local spawns while remote is selected). Nothing rail-specific crosses
the transport.

Rail semantics otherwise follow the established split: **membership, order,
and per-entry scope live in shared state** (announced on the state bus, like
the session/panel pairing) so both frontends agree on what is in the rail;
pane sizing, splits, and collapse state are per-UI presentation
(`tui-layout.json` and a desktop equivalent). **Rail composition is
user-only, with no exception path**: the session-creation rule already
blocks agents from spawning live widgets, and unlike a panel ("look right at
this artifact") there is no agent story for editing an ambient rail the user
curates — an agent that wants attention has the existing attention/status
machinery.

Guardrail pressure doubles here: a persistent rail is where IDE chrome will
knock hardest (git status tiles, file trees, build monitors are the obvious
asks). The panel rule applies with full force — widgets render App surfaces
within the review-surface vocabulary, and the vocabulary keeps refusing
code-workbench content no matter how small the tile.

Phone: the rail does not exist on a phone, and nothing new is needed — fleet
glanceability *is* the phone session list, and the status-row tier already
rides the existing session-list payload (`status.json` carried when it's
worth it, A3's rule). The terminal-first detail rule is untouched.

### Agent-opened panels (MCP)

> Agent access to Apps now has an authoritative contract: the `apps` MCP
> domain in `docs/plans/unpeel-apps.md` (manifest-declared app tools,
> media-type `open`, agent-started sidebar panels, identity/authorization,
> freshness, installed-app search now / catalog later). This section keeps
> only the panel/rendering rules; `open`-style verbs live in that domain,
> not `sessions`.

The ask: an agent says "look right" — it opens an artifact or panel next to
its own session via the `unpeel` MCP (an `apps`-domain action, e.g.
`show_artifact` / `open_app`). Scoping follows the existing trust model:
free in the agent's **own sidebar group** (it is surfacing work where the
user is already looking), with the standard cross-group approval flow
anywhere else.

The session-creation collision is **decided (2026-08-12)**: agents *can*
start an App in the right sidebar. **Session creation stays user-only** (hard
Sessions MCP rule; `create_worktree` prepares only the checkout) with exactly
one bounded exception. The artifact viewer needs none — no session, no
process, just a view command. Starting an *App* in Horizon A spawns a
companion hosted session and therefore rides the shared approval flow (`ask`
by default, remembered pairs, answerable from desktop and phone); the
exception covers only a panel/rail placement paired to the calling agent's
session — never a free-standing session, an agent session, or a full
surface. In Horizon B a panel need not be a session and the approval can
relax to match its real cost. Authoritative contract: the "Agents start apps
in the right sidebar" part of `docs/plans/unpeel-apps.md`.

Phone: columns do not exist on a phone. A panel surfaces as a screen/tab off
the session detail view (the artifact viewer first — the phone already
fetches artifacts scoped); the terminal-first detail rule is untouched.

## Hard guardrails (inherited, non-negotiable)

- **Never IDE chrome; Apps only.** No App — first- or third-party — gets diff
  views, file trees, or editor panes rendered natively. The portable renderer
  is only an Unpeel App surface; it never generates Unpeel's shell, sidebar,
  terminal/session chrome, or a general code-workbench UI.
- **The contract is protocols and files, never a language.** A plugin is
  anything launchable as a command that speaks the documented surfaces: a
  TUI in a PTY, hook events over HTTP (the provider hooks are shell scripts
  today), the `status.json` marker, and — Horizon B — the stdio JSON
  vocabulary. Rust + Ratatui via the `unpeel-ui` crate is the **golden
  path**, not a requirement: first-party plugins use it; third-party
  authors may use any language by speaking the protocols directly.
- **No Node runtime — shipped or required by us.** Unpeel never bundles or
  depends on Node, and no first-party plugin uses it. What a user runs in
  their own environment is their business — a Node-based third-party plugin
  is just a preset command, like any CLI.
- **One remote protocol.** A Horizon B semantic stream reaches the phone over
  the *existing* remote/relay transport (replay-from-offset from the on-disk
  log, live at the tail), never a second channel. A controller must not care
  whether the host is a Mac app or a headless TUI.
- **No app-specific daemon.** A Horizon A app instance is a hosted session
  owned by its host process—manifest, heartbeat, control socket, reap
  discipline. A Room App uses the shared Host RoomFS service and may have an
  optional supervised Host worker when domain logic requires it; it never
  invents a central app daemon or cloud runtime.
- **No new client-side entitlement checks.** Apps are local software on the
  user's machine; nothing local is Link-gated. Link reach is entitled per human
  account and enforced server-side by the shared runtime/service, never by the
  app.
- **The fallback is always a terminal.** If a frontend is too old for a
  protocol version, or an App uses raw/custom Ratatui behavior the portable
  specs cannot represent, the App runs (or re-renders) as a plain TUI in a
  PTY. This is compatibility, not a degraded afterthought; protect it.
- **Standalone first; Unpeel is the superpower, never the requirement.** A
  plugin is a complete CLI tool in any bare terminal — install it, run it,
  it works, no Unpeel present. Running *inside* Unpeel is what adds sidebar
  activity/status, the artifacts panel, preset injection, phone reach, and
  (Horizon B) native rendering. This mirrors how the agent CLIs themselves
  relate to Unpeel, and it is the adoption funnel for a public `unpeel-ui`
  crate: a useful standalone tool that happens to light up when Unpeel is
  around. Never gate a plugin's core function on Unpeel being present, and
  never require Unpeel-only setup steps to first-run a plugin.

## Horizon A — ship plugins now, learn the vocabulary from them

### A1. `unpeel-ui` crate: the style layer, not a framework

A Rust crate on top of Ratatui that makes first-party tools feel like one
family. Extract from `crates/unpeel-tui`, don't invent:

- the color/tint system and status styling (`ui.rs`)
- list/detail layout conventions, the sidebar row idiom
- the fuzzy-scored palette (`palette.rs` — `score` and the item model
  generalize as-is)
- keybinding conventions (`keys.rs`): j/k, ⌘K-alike, fold, help overlay

The crate also owns **environment detection**, so standalone-first is free
for plugin authors rather than a chore: an `unpeel_ui::host()` (or similar)
that reads the env the session host already injects (`UNPEEL_SESSION_ID`,
the hook port). Inside Unpeel, the `status` module posts hook events and
writes markers; outside, every one of those calls is a silent no-op — same
plugin code, zero `if unpeel` branches in the plugin itself. Capabilities
that need the host (panel requests, artifact publishing) degrade to inline
behavior (print the path, render in place), never to an error.

At Horizon A, explicitly **not** in scope: state management, an event bus, or
replacing Ratatui's widget model. Horizon B adds a separate portable owned-spec
layer and adapter in the same crate; raw Ratatui remains available beside it.
If `unpeel-tui` itself adopts the Horizon A style layer, that's a refactor
bonus, not a requirement — do not destabilize the TUI branch before it ships
(shared-core Phase 0 rule).

### A2. `unpeel-tasks`: the proving plugin

A small task/queue tool built on `unpeel-ui`, launched as a preset (it's just
a command — the preset system needs nothing new). It exercises every surface:
desktop Ghostty, TUI attach, phone streaming, archive/restore, the session
gallery. Persistence in a plain file under the project or `~/.unpeel`
(flocked like other shared files if both frontends ever write it).

### A3. Sidebar presence: activity and status messages

Plugins must feel alive in the sidebar the way agent sessions do — a spinner
while working, attention when they need the user, and a short status line
("3 tasks due"). Two mechanisms, deliberately different in reuse:

**Activity (busy/idle/attention) — pure reuse of the hook system.** A plugin
is a hosted session, so it already launches with `UNPEEL_SESSION_ID` and the
hook env. `unpeel-ui` grows a `status` module that emits the *same* canonical
hook events the provider hook scripts do — `UserPromptSubmit`/`Start` (busy),
`Stop` (idle), `PermissionRequest`/`Notification` (attention) — POSTed as
`{"hook_event_name": …}` to `/hook/<session_id>` on **every** port in
`~/.unpeel/app-ports` (multi-instance rule), and mirrored into the durable
`last-hook-event.json` seed so the latch survives frontend restarts. Both
activity engines (`SessionActivity.swift`, `activity.rs`) then work
unmodified: the first event latches the session hook-owned, output heuristics
stop mattering, unread badges integrate for free. No new event names — the
existing vocabulary maps cleanly and inventing plugin-specific ones would
fork both engines.

**Status text — a new, small shared surface.** Nothing today renders
free-text status in a sidebar row; do it with the established shared-state
pattern, not a new channel: a per-session `status.json` marker in the session
dir (like `title.json`), written by the plugin via `unpeel-ui`, announced on
the state bus (`state_bus::announce` — flush before exit for one-shot
writes), rendered by both sidebars as a secondary line/suffix on the row.
Short, single-line, plain text — a status, not a log. Phone: carry the field
in the existing session-list payloads when it's worth it; never a second
transport. Agent sessions could later reuse the same marker (e.g. a hook
script surfacing the last Notification text), which is the tell that this
belongs in shared state rather than inside the plugin protocol.

**Write semantics — no queue, by design.** Status is per-session and each
plugin is its own session, so plugins never contend for one file. Where
writers could overlap on a single session (the plugin process plus a future
hook-script writer), the model is atomic whole-file overwrite,
last-writer-wins: a status is ephemeral "what's true now", not a log —
queueing stale statuses behind fresh ones would be wrong. Bus announces
carry no payload (they mean "re-read"), so bursts coalesce at the listener
for free. The crate debounces writes (a few per second at most) so a chatty
plugin can't treat the sidebar like a progress bar.

Rules inherited: writers announce on the bus (ping is an optimisation,
polling still catches it); single-owner file so no flock needed; the hook
server keeps answering 404 for unknown session ids, which already keeps
foreign instances from swallowing plugin events.

### A4. The portability notebook

While building A2 (and any second plugin), keep a running list: which
interactions were *semantic* (list, select, toggle, form field, confirm,
progress), which Ratatui options need native meaning, and which raw/custom
operations cannot cross the protocol. This notebook prioritizes built-in
coverage and exposes adapter gaps; it no longer defines a separate tiny
vocabulary. This is the cheapest way to validate B against real Apps.

### A5. Distribution: plugins auto-inject into the preset list

A plugin the user installs should just appear — no wizard, no import step.
The mechanism already exists: the startup PATH scan that seeds builtin
presets. Extend it to recognize plugins (an `unpeel-*` binary naming
convention to start; a manifest probe if that proves too loose) and seed a
preset entry through the **sanctioned write paths only** — `app_state::edit()`
on the Rust side, `editPresetStateAnnouncing` in the app — landing in the one
flat `presets` array in `app-state.json`, bus-announced like any preset edit.
Never a second preset store or overlay.

Injection is a **seed, not an assertion**: after the one-time inject, the
user owns the entry completely — reorder (which is choosing the default),
star it into a sidebar chip, hide it. A plugin preset the user removed or
hid must not resurrect on the next scan, so remember seeded plugin ids in a
tombstone the way removed projects are remembered. Uninstalling the binary
makes the preset launch-dead like any preset whose command left the PATH —
no special casing.

**Horizon A exit criteria:** two first-party plugins shipped and used, with
sidebar activity/status live in both frontends and preset auto-injection
working end to end; the portability notebook has exercised enough built-in
Ratatui surface to prioritize the portable adapters.

## Horizon B — the semantic protocol

### What it is

A plugin process speaks JSON over stdio: it emits a declarative tree of
**portable Ratatui-shaped owned specs** and receives **semantic** actions
back. These are Unpeel-owned serializable values and builders modeled closely
on Ratatui's built-in concepts and option names, not Ratatui's borrowed widget
structs serialized after rendering. Each frontend owns presentation. Sidebar
activity and status text stay on the
Horizon A path (hook events + `status.json`) in both horizons — they are
session-level state every frontend already syncs, not part of the plugin's
rendered surface, and keeping them out of the widget protocol means a
PTY-fallback plugin loses nothing in the sidebar.

- **Coverage target:** Ratatui's built-in concepts and public options: text,
  spans and styles; blocks and borders; paragraphs, lists, tables and tabs;
  gauges, charts, sparklines, scrollbars and calendars; layout direction,
  constraints and alignment; plus a recording canvas whose points, lines,
  rectangles, circles, labels, and layers are explicit data. Coverage lands
  incrementally and is versioned, but the API deliberately
  feels like Ratatui rather than a second unrelated UI vocabulary.
- **Terminal meaning:** the Rust adapter constructs real Ratatui widgets from
  the specs. Terminal rendering is the fidelity reference for Ratatui-specific
  options.
- **Native/web meaning:** SwiftUI and a future web renderer interpret the same
  node as the corresponding native control/drawing primitive. They preserve
  content, state, actions, and accessibility; they need not reproduce cell
  geometry or terminal-only decoration pixel for pixel.
- **Rust remains the backend:** the running CLI owns its model, validation,
  commands, persistence, and business rules. Native/web clients only render
  snapshots and return semantic actions; they never reimplement App domain
  logic in Swift or JavaScript.
- **Interaction:** every interactive node has a stable string id and names
  stable semantic actions such as select, submit, cancel, or activate. Clients
  return those actions and typed values—not key codes or pointer coordinates.
  Reorder is a generic collection behavior for stable item ids: cards, list
  items, tabs, table rows, and table columns all map renderer-specific
  keyboard/mouse/drag gestures to one ordered-id action. Manual reorder stays
  distinct from data sorting.
  Protocol v1's canvas is presentational; a later version may add explicit
  semantic hit regions, but raw pointer traffic never becomes the App
  contract.
- **Update model:** v1 emits a complete revisioned snapshot after each state
  change. Tree patches/diffs may be added later if measurement justifies them;
  they are not a prerequisite or an alternate first protocol.
- **Compatibility boundary:** an arbitrary third-party `impl Widget`, direct
  `Buffer` mutation, or closure/custom paint code has no recoverable native
  semantics. It remains fully supported in raw Ratatui/PTY mode. An unknown or
  too-new portable node/option similarly renders a clear text/update fallback
  or causes the whole App to use its PTY view. Version skew is a first-class
  case, like the `compat_*` TUI tests.
- **Scope:** this protocol belongs only to Unpeel Apps. It is never a way to
  generate Unpeel's own shell or to bypass the no-IDE-chrome product rule.

### Why it converges with dual-mode sessions

`docs/plans/dual-mode-sessions.md` already defines the machinery Horizon B
needs: a **structured session kind** in `session_host.rs` — child process
speaking stdio JSON, append-only NDJSON event log instead of `output.bin`,
structured control-socket commands, replay-from-offset for remote clients.
A UI-mode agent session and a Horizon B App share Host supervision,
validation, replay, and controller routing, with different vocabularies on top
(conversation events vs. widget tree). Structured-only children use JSON
stdio. A hybrid App keeps its PTY and carries the same widget protocol over an
auxiliary session channel; see `unpeel-app-native-rendering.md`.

The account-backed Rooms direction adds a lower, app-agnostic sharing layer:
a scoped Host **RoomFS** with revisioned file operations and change cursors.
RoomFS is durable shared UI state/transport; it does not replace the semantic
widget protocol. A Horizon B app can read/write its schema through RoomFS and
render it on every client, while an optional Host process can still validate
domain commands or emit a semantic tree. The default Apps SDK above RoomFS is
RoomStore: filesystem-backed collections, append logs, per-user state,
ephemeral presence, blobs, and atomic transactions. See
`account-backed-rooms.md`.

Build that mechanism **once**. Whichever plan gets there first implements it;
the other consumes it. What Horizon B adds on top is only: the portable
spec/action contract, its renderers, and the App manifest (name, command,
protocol version).

### Renderer drift: the part that actually decides success

Sharing a protocol does not stop its renderers from diverging — testing
them against one expectation does (the shared-core lesson, one layer up).
From day one: a corpus of golden widget trees, and a conformance pass that
renders each through the Ratatui renderer (assert on the character grid, the
snapshot harness in `crates/unpeel-tui/tests` already does this) and through
the SwiftUI renderers (the app's Snapshot harness). A tree that renders
wrongly on any frontend fails CI, not a user. No renderer merges a new
widget without its golden cases.

### Sequence

1. **B1a — package foundation (started; independent of Host integration).**
   The first `unpeel-ui` slice has versioned owned specs, full-snapshot and
   semantic-action envelopes, JSON fixtures/schema, validation, and the
   Ratatui adapter. List/Table state and the generic reorder contract/helper
   are also built. Continue toward built-in Ratatui coverage and
   cross-renderer goldens here.
2. **B0 — Host integration prerequisite:** dual-mode Phase 1 (structured
   session kind + NDJSON log) exists and has shipped in some form. Library
   work above does not need it; launching and replaying a semantic App does.
3. **B1b — proving App + TUI integration.** `unpeel-todos` gains a semantic
   mode; the TUI renders it through the Ratatui adapter and sends semantic
   actions back. One frontend, real usage, golden corpus expanded.
4. **B2 — SwiftUI renderer (Mac).** The app renders semantic App sessions
   natively, including the optional stacked-native-view-above-terminal
   composition defined in `unpeel-app-native-rendering.md`; PTY fallback is
   verified by collapsing or disabling it. Feature-flagged
   (`ExperimentalFeature`).
5. **B3 — phone.** Widget-tree log streamed over the existing remote/relay
   transport; iOS SwiftUI renderer. Terminal-first detail view is untouched
   for terminal sessions — semantic Apps are a sanctioned native surface,
   like dual-mode chat.
6. **B4 — web renderer, when an Unpeel web controller exists.** The same
   fixture corpus drives accessible DOM/SVG/canvas output; no App-specific web
   transport is introduced.
7. **B5 — third-party Apps, maybe.** Only after first-party Apps prove the
   contract. Requires the trust story below; distribution is "a binary
   on your PATH + a manifest", never a store or a cloud tier.

## What stays split, on purpose

Presentation. Fonts, colors, spacing, animation, focus order, which native
control implements a portable concept (SwiftUI `List` vs. Ratatui rows), and
keyboard vs. touch affordances. Specs carry the Ratatui options needed for
meaning and terminal fidelity; clients may adapt terminal-specific decoration
to platform conventions. An App says *what*; each frontend decides *how it
looks and feels* — that asymmetry is the whole design.

## Open questions / risks

- **Trust boundary for third-party plugins.** A plugin is an arbitrary local
  process, like any preset command — but if plugins ever get verbs against
  other sessions, they go through the Sessions MCP trust model
  (open reads / same-group writes / cross-group approval), not a new one.
- **Where plugins live in the sidebar.** A session under a project? A
  distinct section? Defer until A2 makes it concrete; whatever the answer,
  both frontends read it from shared state.
- **Parity surface and renderer drift.** Broad built-in Ratatui coverage is
  the target, but every concept/option needs fixtures and meaningful adapters.
  Add coverage incrementally behind protocol capabilities; never pretend an
  arbitrary custom `Widget` is portable. PTY fallback means missing native
  coverage never blocks an App from existing.
- **Ratatui API stability** in the public `unpeel-ui` crate: pin the terminal
  adapter/re-export, while keeping the versioned owned wire specs independent
  of borrowed Ratatui internals. The open-source boundary is decided, not
  conditional.

## Related

- `docs/plans/unpeel-app-native-rendering.md` — one Rust App process driving a
  native SwiftUI view above its live terminal
- `docs/plans/unpeel-link.md` — canonical Link service contract beneath Apps
- `docs/plans/unpeel-apps.md` — authoritative Apps SDK/API, Apps UI SDK,
  Activity, and RoomStore defaults
- `docs/plans/dual-mode-sessions.md` — the structured session kind Horizon B
  rides on; build that mechanism once
- `docs/plans/shared-core.md` — the drift lesson (test against one
  expectation) and the Phase 0 "don't destabilize the TUI branch" rule
- `docs/plans/headless-host.md` — why the semantic stream must ride the one
  shipped remote protocol
- `docs/plans/host-controller-transports.md` — the host-authoritative semantic
  event stream and relay/direct/SSH transport boundary
- `docs/plans/account-backed-rooms.md` — Host RoomFS beneath shared app UIs;
  publication never becomes cloud data storage
- `docs/plans/open-source.md` — `unpeel-ui`, RoomStore, all renderers/clients,
  and their protocols are open; only the operated Link backend is closed
- `crates/unpeel-tui/src/{ui,palette,keys}.rs` — the extraction source for A1
