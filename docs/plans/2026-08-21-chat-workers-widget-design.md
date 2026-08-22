# Chat Workers Widget Design

## Goal

Replicate the Orchestrator.dev `Workers` details widget in Comet. The widget must
present structured workflows, runtime subagents, and terminal-backed CLI workers
that belong to the selected chat. Rows remain useful after work settles, and a
CLI worker opens its already-running terminal in the chat split instead of
navigating away or launching a duplicate process.

## Reference behavior

The reference widget is a three-tab card in the Details sidebar:

- `Workflows` contains named or multi-phase orchestration tasks. A row can expand
  into phase and agent progress, and displays usage and duration when available.
- `Subagents` contains genuine delegated agent tasks, not ordinary background
  shell commands.
- `Workers` contains terminal-backed CLI tasks associated with the current chat.
- Each tab shows a count, active work sorts before settled history, and the list
  becomes scrollable after roughly five compact rows.
- Running work animates, blocked work is amber, successful work is green, failed
  work is red, and recovery/disconnected are explicitly distinct from success.
- Clicking a subagent opens its transcript. Clicking a CLI worker opens the exact
  terminal-backed session in an adjacent split while the parent chat remains
  visible.

## Chosen architecture

Comet will use a native, chat-scoped activity projection rather than parsing
rendered transcript text or porting the Electron/Jotai store.

### Normalized runtime activity

`zeron-proto` gains additive workflow-task lifecycle data with stable task IDs,
status, optional workflow name and description, usage, progress nodes, agent
count, task type, and subagent type. Harnesses emit this data only when their
wire protocols provide it:

- Claude Code maps `task_started`, `task_updated`, `task_progress`, and
  `task_notification` frames.
- OMP maps subagent lifecycle/progress and any structured workflow snapshots.
- ACP/Codex retain their existing subagent events and may populate workflow
  lifecycle data when the upstream protocol exposes equivalent metadata.

Ordinary background shell tasks must not appear as subagents.

### Durable chat projection

The engine folds normalized activity into bounded, additive chat document state.
Updates merge by task ID so lightweight lifecycle frames do not erase richer
progress snapshots. The snapshot survives reload and device synchronization.
The projection retains at most 100 settled tasks per chat while never evicting
active tasks. The UI reads one typed snapshot instead of independently
interpreting provider-specific events.

Subagent transcript identity and lifecycle continue to use the existing
`subagent_ref`, `subagent_status`, and `subagent_tail` document fields. The widget
projects those durable spawn chips into the common presentation model.

### CLI worker association

`zeron-workers-unpeel` exposes a read-only snapshot of persisted
`worker session -> parent chat` bindings. It does not expose mutation authority or
notification internals. `WorkersModel` joins those bindings with its current
session snapshot and publishes only workers whose parent chat is the active chat.

The semantic status ladder is exhaustive:

1. starting/working;
2. blocked/waiting for input;
3. terminal success/failure/cancelled;
4. idle;
5. recovery;
6. disconnected.

Missing completion evidence must never be rendered as success.

## UI composition

A focused `chat_workers` module owns presentation types, grouping, sorting, and
the GPUI widget. `DetailsSidebar` receives the shared `WorkersModel`, reads the
selected chat's activity snapshot, and renders the card only when at least one of
the three categories has data.

The card matches the reference hierarchy:

- standard Details widget header with Workers glyph and `Workers` title;
- compact tab strip with `Workflows`, `Subagents`, and `Workers` counts;
- selected tab uses the existing subtle ink fill;
- a maximum-height, vertically scrollable body;
- active rows first without reordering equal-status rows on every stream tick;
- workflow rows expand independently and preserve their identity across status
  bucket changes;
- empty selected tabs show a quiet `No ... yet.` state.

The widget follows Comet theme tokens and reduced-motion settings. It reuses the
provider icons already used by runtime and transcript surfaces.

## Split behavior

Subagent rows reuse the existing `OpenSubagent` path and transcript surface.

Worker rows emit a typed `OpenWorkerSession` event carrying the stable worker
session ID. `Shell` creates or focuses a right-side worker terminal surface bound
to that exact `WorkersSession`. The surface attaches through the existing
Workers PTY/session contract; it must not call launch, restart, or create a new
runtime. Repeated clicks focus the existing surface. Closing it detaches only the
view and leaves the worker lifecycle untouched.

If the session disappeared between render and click, the widget refreshes the
Workers snapshot and shows a quiet unavailable state rather than opening a
different terminal.

## Bounds and lifecycle

- Chat activity is keyed by chat ID and task ID.
- At most 100 settled workflow/subagent activity rows are retained per chat;
  active rows are exempt until they settle.
- Workers are not copied into chat documents; the widget joins the current
  Workers snapshot against durable bindings.
- Status updates are idempotent and merge richer fields rather than replace
  them with absent values.
- A selected tab is local view state; when never selected, the first non-empty
  tab wins in the order Workflows, Subagents, Workers.

## Error handling

- Malformed provider progress is ignored without breaking the agent stream.
- Unknown status strings normalize conservatively to running only while there is
  live evidence; otherwise they become recovery/disconnected.
- Binding parse errors do not hide workflow/subagent data and surface through the
  existing Workers error channel.
- Missing transcript or worker sessions render disabled/unavailable rows.
- No inferred success is allowed after process loss, reload, or stale state.

## Test strategy

TDD will cover:

1. Claude and OMP event normalization, including terminal and malformed frames.
2. Incremental task merge semantics and the bounded settled-history policy.
3. Subagent extraction without admitting ordinary background shell tasks.
4. Read-only worker binding snapshots and chat filtering.
5. Active-first stable ordering and exhaustive semantic status mapping.
6. Split identity, focus-on-repeat, and view-only close behavior.
7. GPUI widget tabs, counts, expansion, empty states, and unavailable rows.
8. Focused crate tests, workspace check/build, and side-by-side native visual
   validation against Orchestrator.dev.

## Non-goals

- Porting React, Jotai, TRPC, or Electron implementation details.
- Treating generic Bash/background commands as subagents.
- Replacing the existing full Workers workspace.
- Starting, restarting, or terminating a worker as a side effect of opening a
  widget row.
- Adding worker management actions to this compact widget.
