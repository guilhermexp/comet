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
- Preserve the existing specialized rows, tool detail folds, inline images,
  Mermaid rendering, todo cards, reasoning, and subagent links inside the
  expanded disclosure.
- Preserve transcript virtualization, stable row identity, minimal splices,
  bottom pinning, own-turn runway, and render caches.

## Non-goals

- Replacing or visually redesigning the composer.
- Changing runtime protocols, durable document schemas, or harness adapters.
- Collapsing the user's message bubble with the assistant work.
- Introducing a second final-answer field into persisted messages.
- Hiding active subagents or unresolved input requests.
- Adding a new response label, duration field, or animation system.

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
- no durable schema or runtime change.

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

## Architecture

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
2. reads;
3. searches;
4. edits;
5. commands;
6. tools.

Mappings:

- agent-shaped MCP/unknown calls -> agents;
- `ReadFile`, `Search`, and `Glob` -> reads;
- `WebSearch` and `WebFetch` -> searches;
- `WriteFile`, `EditFile`, and `ApplyPatch` -> edits;
- `Exec` -> commands;
- `Todo`, other MCP calls, and other unknown calls -> tools.

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
}
```

The composite row id is `{entry.id}#steps`. Its version hashes the mode,
summary, and every child id/version. Prefix `LiveMarkdown` children become
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

### Cache and lifecycle safety

- `invalidate_row_tree` invalidates both the composite id and every child id
  when a `TurnSteps` row is replaced.
- `turn_steps_open` is pruned against current composite row ids after sync.
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

## Validation

Focused pure tests cover boundary and summary policy. Transcript projection
tests cover stable ids, cross-narration grouping, streaming movement, final
answer separation, running subagents, todo cards, and child default folds.
Render-state tests cover independent disclosures and state survival through row
replacement. Final validation runs the full UI suite, native build, formatting,
diff hygiene, and a real Claude/Codex/OMP smoke in the rebuilt app.

