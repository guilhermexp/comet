# Assistant Turn Steps Design

## Problem

Comet currently folds consecutive ordinary tools into independent `ToolGroup`
rows. Text, reasoning, todo snapshots, input requests, and agent tools split
those groups. A single assistant turn that narrates between calls therefore
leaves several unrelated fold headers in the transcript and never acquires the
single operational summary used by Orchestrator.dev.

Orchestrator.dev treats one user message plus its assistant replies as a turn,
then applies the disclosure inside each assistant message. During streaming it
folds the completed operational prefix while keeping all unsettled top-level
activity visible. After settlement it folds everything before a final text
segment that appears after the last tool, leaving the final answer expanded.

The composer is a separate concern. Comet's native composer already owns its
compact-to-expanded morph, context-window ring, attachments, comments, question
wizard, runtime selectors, and Send/Steer/Stop behavior. This change must not
replace or restyle it.

## Goals

- Add one stable `TurnSteps` disclosure per eligible assistant message.
- During streaming, keep every unresolved activity and the latest visible
  activity outside the disclosure.
- After settlement, keep the final answer outside the disclosure when it comes
  after the turn's last tool.
- Summarize the whole folded prefix across narration boundaries, for example
  `9 reads, 10 edits, 4 commands, 2 tools`.
- Show the persisted duration of the assistant message at the right edge of
  the disclosure header once the segment reaches a terminal state.
- Preserve the existing specialized rows, tool detail folds, inline images,
  Mermaid rendering, todo cards, reasoning, and subagent links inside the
  expanded disclosure.
- Preserve transcript virtualization, stable row identity, minimal splices,
  bottom pinning, own-turn runway, and render caches.

## Non-goals

- Replacing or visually redesigning the composer.
- Changing external runtime protocols or inventing file contents a runtime did
  not expose. Harness normalizers may preserve deltas already present upstream.
- Collapsing the user's message bubble with the assistant work.
- Introducing a second final-answer field into persisted messages.
- Hiding active subagents or unresolved input requests.
- Adding a new response label or animation system.
- Adding a live elapsed timer to a disclosure whose segment is still running.

## Considered approaches

### 1. Composite `TurnSteps` row (recommended)

Project the completed prefix into one `RowKind::TurnSteps` row. Its children
retain their original stable row ids and renderer types. The top-level list sees
one row for the folded prefix, while expansion renders the existing child
components inside it.

Advantages:

- matches Orchestrator's semantic unit;
- closed disclosures do not mount their child subtree;
- child tool folds and media renderers remain reusable;
- list splices operate on one stable `entry#steps` row;
- no runtime or harness change; one additive durable entry field carries time.

Cost: `Transcript::render_row` must be split into outer list-row chrome and an
inner row-body renderer so the composite can reuse child renderers without
nesting list wrappers.

### 2. Flat rows plus a summary sentinel

Keep every existing row top-level and conditionally render prefix rows as zero
height behind a summary row.

Rejected because hiding variable-height rows undermines GPUI list measurement,
scroll anchors, own-turn reservation, and minimal remeasurement. Filtering rows
on every toggle would also make a local disclosure choice mutate the entire
virtualized row projection.

### 3. Extend `ToolGroup` across narration

Allow one tool group to span intervening text/reasoning.

Rejected because `ToolGroup` is not a turn model. It cannot identify the final
answer, would either discard or duplicate narration, and would force todo,
reasoning, input, and media components into a tool-only abstraction.

## Duration source

Comet's engine already owns the authoritative start time for each assistant
segment in `drive_run.segment_started`. That boundary restarts for a routed
steer and for a parked persistent-session turn, exactly where a new assistant
message entry begins. The elapsed time therefore belongs on
`SessionMessageEntry`, not on a tool, runtime session row, or renderer timer.

### Considered duration approaches

1. **Persist engine-measured segment duration (recommended).** On every normal,
   interrupted, errored, quiesced, steered, and subagent segment finish, store
   `max(0, finished_at - started_at)` as `duration_ms` on the assistant entry.
   This works for every runtime and survives restart/sync.
2. **Derive from session `started_at`/`updated_at`.** Rejected because the
   session row is mutable, chat-scoped, cleared on idle, and not historical per
   assistant message.
3. **Use provider-reported duration.** Rejected as the authority because the
   field is not universal and different providers measure different intervals.
   Provider tool durations remain tool metadata; they do not define turn time.

`SessionMessageEntry.duration_ms: Option<u64>` is an additive Loro/JSON field.
Old entries read as `None`. Recovery paths that merely stamp a stale streaming
entry terminal and cannot reconstruct its real start/end leave it `None` rather
than inventing a duration.

## Architecture

### Incremental file-change cards

`WriteFile` and `EditFile` gain a specialized transcript presentation. They do
not remain generic invocation code blocks when a file preview is available.
The card lifecycle matches Orchestrator:

- input starting without a path -> minimal `Creating`/`Editing` shimmer;
- path/content arriving -> full card, filename shimmer, spinner, green/red
  bounded preview;
- resolved -> `+N/-N`, expand/collapse control, syntax highlighting;
- collapsed -> fixed 72px preview;
- expanded -> at most 200px with an independent vertical scrollbar and the
  complete historical tool input when available.

The filename remains a separate open-file action. Header/body/chevron toggle
the inline preview only after resolution; active writes cannot be manually
expanded while their input is still changing.

#### Considered file-content approaches

1. **Bounded durable preview plus journal-backed full expansion
   (recommended).** Repeated ToolCall refreshes update a bounded 15-line
   semantic preview in the doc. Expanding a completed card fetches the original
   unsanitized Write/Edit input from the host run journal by chat/tool id. This
   preserves historical correctness without replicating whole files.
2. **Reuse generic `ToolDetail::Output`.** Rejected because it has neutral
   output semantics, a 24-line truncation, no file lifecycle, and no independent
   inner scroll surface.
3. **Persist complete Write/Edit input in `SessionMessageEntry`.** Rejected
   because the existing privacy policy deliberately strips file bodies from
   synchronized docs; large generated files would restore the doc-size failure
   that the policy prevents.

#### Progressive normalization

No new `AgentEvent` variant is required. `fold_event_into_parts` already
refreshes an existing tool when another `AgentEvent::ToolCall` arrives with the
same id. Normalizers use that contract:

- Claude extends its stream-event wire shape with content-block index,
  tool-use start metadata, and `partial_json`; it accumulates only the active
  tool input and emits refreshed typed Write/Edit calls as path/content fields
  become decodable;
- OMP tracks `toolcall_start|delta|end` by content index and performs the same
  refresh before the existing authoritative `tool_execution_start` call;
- ACP already emits shape-bearing `tool_call_update` refreshes and needs
  regression coverage, not a second path;
- Codex remains path-only when its `fileChange` item exposes no content. It gets
  the same card header/lifecycle but no fabricated body.

A shared harness helper performs focused partial extraction for JSON string
fields (`path|file_path`, `content`, `old_string|oldText`,
`new_string|newText`). It decodes complete JSON escape sequences and returns
the safely decoded prefix of an unterminated string. It is not a general JSON
repair parser and never evaluates arbitrary JSON.

Engine doc commits already coalesce at `STREAM_COMMIT_MS == 120`, so the native
preview updates at approximately the reference's 100ms cadence without a
second renderer timer.

#### Bounded durable preview

`MessagePart::Tool` gains:

```rust
pub enum FileChangeKind { Write, Edit }
pub enum FileChangeLineKind { Added, Removed, Context }

pub struct FileChangeLine {
    pub kind: FileChangeLineKind,
    pub text: String,
}

pub struct FileChangePreview {
    pub kind: FileChangeKind,
    pub lines: Vec<FileChangeLine>,
    pub total_lines: u32,
    pub additions: u32,
    pub deletions: u32,
    pub truncated_before: u32,
}
```

The fold derives this preview before `sanitize_tool_call` removes full content.
Write treats every line as added. Edit uses `similar::TextDiff` when both sides
exist and an authoritative `ToolDiff` when the result supplies one. The doc
retains at most the last 15 display lines, each capped at 512 Unicode scalar
values. Counts cover the complete available input, not only retained lines.
The preview is additive/backward-compatible and participates in part byte
length, schema, fingerprints, transcript deltas, and row versions.

Green means `Added`, not execution success. Tool lifecycle continues to own
spinner/success/failure independently from diff colors.

#### Historical full-input fetch

Add relay-forwardable `FetchToolInput { chatId, toolCallId }`. The target
engine scans that chat's append-only run journal newest-first for the matching
`AgentEvent::ToolResult.diff` or `AgentEvent::ToolCall` and returns only a
sanitized file-input snapshot:

```rust
pub struct FileToolInputSnapshot {
    pub path: String,
    pub content: Option<String>,
    pub old_string: Option<String>,
    pub new_string: Option<String>,
    pub truncated: bool,
}
```

The response is capped at 1 MiB and never returns arbitrary MCP/unknown inputs.
The UI includes the chat's `targetDeviceId`, so a synced client asks the device
that owns the journal. Missing/compacted journals leave the bounded preview in
place and show a retryable `Full content unavailable` row; the UI does not read
the current workspace file as a historical substitute.

`Transcript` caches Loading/Ready/Failed fetch state and one `ScrollHandle` per
expanded file row. Closed cards do not fetch. Reopening a Ready card performs
no RPC and preserves its internal scroll position until the row/chat is
removed.

### Pure turn policy

A new `crates/ui/src/turn_steps.rs` module owns provider-neutral decisions over
`MessagePart`:

```rust
pub enum TurnStepsMode {
    StreamingPrefix,
    FinalAnswer,
}

pub struct TurnStepsPlan {
    pub split_before_part: usize,
    pub mode: TurnStepsMode,
    pub summary: String,
}

pub fn plan_turn_steps(
    parts: &[MessagePart],
    status: Option<MessageStatus>,
) -> Option<TurnStepsPlan>;
```

The policy does not construct GPUI elements and has no dependency on
`Transcript` state.

#### Settled assistant message

Find the last tool part and the last non-empty text part. Create a plan only
when both exist and the text comes after the tool. The split is before that
text. Messages with no tools, no final text, or narration only before tools do
not gain a turn-level disclosure.

#### Streaming assistant message

Scan visible parts after the first part:

1. If an unresolved tool, running subagent, active reasoning part, or unresolved
   input exists, split immediately before the first such activity.
2. Otherwise split immediately before the latest visible part.
3. Return no plan when the split would be at index zero or the prefix has no
   visible operational content.

The boundary is before the first unsettled activity, so concurrent unresolved
activities remain together outside the disclosure. A subagent with
`subagent_status == Running` is unsettled even when the spawn tool itself has
already resolved.

### Activity breakdown

The same pure module summarizes tool calls in the prefix in this fixed order:

1. agents;
2. skills;
3. reads;
4. searches;
5. edits;
6. commands;
7. waits;
8. messages;
9. terminals;
10. captures;
11. tools.

Mappings:

- agent-shaped MCP/unknown calls -> agents;
- `Skill`/`skill` calls -> skills;
- `ReadFile`, `Search`, and `Glob` -> reads;
- `WebSearch` and `WebFetch` -> searches;
- `WriteFile`, `EditFile`, and `ApplyPatch` -> edits;
- `Exec`, `Eval`, and terminal-creation calls -> commands;
- peer/background-job waits -> waits;
- agent prompts and terminal-key sends -> messages;
- terminal listing -> terminals;
- terminal capture -> captures;
- `Todo`, other MCP calls, and other unknown calls -> tools.

OMP `hub` calls are classified by their `input.op`: `wait` -> waits, `send` ->
messages, `start|restart|stop` -> commands, and every other operation -> tools.
Built-in and MCP calls feed the same `ActivityBucket` counts, so the Comet
header does not reproduce Orchestrator's duplicate labels such as
`1 agent, ..., 1 agent`.

Reasoning, text, workflow-only updates, inline-image projections, and input/error
parts do not increment a bucket. If no tool is categorizable, the header falls
back to `N steps`, using the count of visible projected child rows.

### Row projection

`rows_for_entry_with_todo_history` will build private `ProjectedRow` values:

```rust
struct ProjectedRow {
    source_start: usize,
    source_end: usize,
    row: Row,
}
```

The source range keeps the part-to-row relationship explicit for Markdown
blocks, tool groups, inline image galleries, todo snapshots, reasoning, input,
and errors. When a planned split lands before a tool, the pending tool group is
flushed first so a group never straddles the boundary.

Rows wholly before the split become children of:

```rust
RowKind::TurnSteps {
    rows: Arc<Vec<Row>>,
    summary: SharedString,
    duration_ms: Option<u64>,
}
```

The composite row id is `{entry.id}#steps`. Its version hashes the mode,
summary, `duration_ms`, and every child id/version. Prefix `LiveMarkdown` children become
settled `Markdown` children, and prefix `ToolGroup` defaults are forced closed;
the prefix is already complete even while its owning entry continues streaming.
Explicit child fold choices still win through the existing stable child ids.

### Rendering and disclosure state

Extract the current `RowKind` match from `render_row` into
`render_row_body(&Row, ...)`. `render_row` continues to own list gaps, gutters,
hover timestamps, trailers, and turn-start behavior. `TurnSteps` calls only the
body renderer for its children, preventing nested transcript gutters and
timestamp strips.

`Transcript` owns:

```rust
turn_steps_open: HashMap<SharedString, bool>
```

The default is closed. The key is `{entry.id}#steps`, so the choice survives row
remeasurement and virtualized remounts but resets on a chat switch, matching the
existing render-local fold policy. Closed disclosures do not render children.
Expanded disclosures show the existing child components with a six-pixel
vertical gap. The outer disclosure follows Orchestrator and toggles without a
variable-height tween; nested tool/detail folds retain their existing analytic
animations.

The header renders `duration_ms` only when it is `Some` and greater than zero,
using Orchestrator's compact formatter: `<1000ms` -> `Nms`, `<60s` -> one
decimal second, and `>=60s` -> `Nm Ns`. Duration is quiet, monospaced, and
right-aligned immediately before the disclosure chevron.

### Cache and lifecycle safety

- `invalidate_row_tree` invalidates both the composite id and every child id
  when a `TurnSteps` row is replaced.
- `turn_steps_open` is pruned against current composite row ids after sync.
- `duration_ms` participates in entry fingerprints, transcript delta equality,
  and composite row versions, so the terminal metadata update remeasures only
  its owning disclosure row.
- Child ids remain unchanged when moving into or out of the disclosure.
- Streaming-to-settled transitions preserve `{entry.id}#steps` whenever the
  message remains eligible.
- Timestamp ownership stays on the final top-level row, never on a folded child.
- The current unresolved input, active reasoning, running subagent, and final
  answer remain top-level rows, so own-turn height and bottom pinning continue
  to measure visible work honestly.

## Error and edge behavior

- Aborted turns without a final text use existing individual folds; Comet does
  not invent a final-answer boundary.
- A final text before the last tool is narration, not a final answer.
- An unresolved input request is never folded.
- A running subagent is never folded, including eager-resolved spawn tools.
- Completed/failed subagents may enter the prefix and count as agents.
- Empty/whitespace text and `WorkflowTask` parts do not create boundaries.
- A split with no visible prefix produces no disclosure.
- Expanding an old disclosure cannot restart a live Markdown veil.
- A live Write/Edit row remains outside `TurnSteps` as an unsettled tool. After
  resolution it may enter the turn prefix; expanding the outer steps disclosure
  restores the specialized file card and its independent inner fold state.
- A runtime that reports only a path renders an honest path-only file card; it
  never receives invented green lines or counts.
- Old entries and recovery-finalized entries without a trustworthy elapsed time
  omit the duration label.

## Validation

Focused pure tests cover boundary and summary policy. Transcript projection
tests cover stable ids, cross-narration grouping, streaming movement, final
answer separation, running subagents, todo cards, and child default folds.
Render-state tests cover independent disclosures and state survival through row
replacement. Final validation runs the full UI suite, native build, formatting,
diff hygiene, and a real Claude/Codex/OMP smoke in the rebuilt app.
Schema/engine tests additionally cover completed, interrupted, errored,
quiesced, steered, subagent, legacy, continuation, and recovery duration paths.
Harness/doc/UI tests cover partial Write/Edit refresh, bounded preview privacy,
journal fetch authorization, 72px/200px disclosure geometry, inner scrolling,
and composition with the outer turn disclosure.
