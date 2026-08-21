# Orchestrator Streaming and Tool Block Parity Design

## Goal

Bring the Orchestrator.dev chat lifecycle into Comet's native GPUI transcript:
ordinary tools show live progress, command cards transition to honest success or
failure, active command details remain visible while the turn streams, invocation
and result use distinct tonal hierarchy, supported runtimes preserve execution
metadata, and reasoning renders as a dedicated Thinking/Thought component.

The implementation must preserve Comet's existing runtime-neutral `AgentEvent`
pipeline, Loro document compatibility, transcript virtualization, block-granular
Markdown parser, tool grouping, sidecar policy, contextual icons, and session
behavior.

## Existing foundation

Comet already owns the difficult structural pieces:

- every harness emits normalized `AgentEvent::ToolCall` and
  `AgentEvent::ToolResult` events;
- the doc fold refreshes tools by stable id and records `resolved`/`is_error`;
- the trailing ordinary tool group opens only while its entry streams and closes
  when the entry settles;
- group and detail folds have analytic heights, virtualized-list-safe state, and
  user-toggle animations;
- tool invocation, bounded output summaries, diff stats, inline diffs, and lazy
  sidecar upgrades already have typed renderer models;
- Markdown is append-incremental, split into stable top-level rows, syntax
  highlighted paint-only, and protected against incomplete markup reflow;
- contextual tool icons are already shared across every runtime.

This work extends those seams instead of introducing a parallel transcript or
runtime-specific UI.

## Decisions

### 1. Keep one normalized tool lifecycle

`ToolCall` remains the start/update event and `ToolResult` remains the terminal
event. Do not add a universal output-delta protocol in this change. Codex exposes
command output deltas, but Claude and OMP commonly expose tool output only at
completion; forcing every adapter into a richer streaming contract would add
state with no portable meaning.

The UI derives progress from the existing truth:

- `resolved == false` means the tool is in progress;
- `resolved == true && is_error == false` means it completed successfully;
- `resolved == true && is_error == true` means it failed.

### 2. Centralize lifecycle copy and outcome

Extend `zeron_proto::view` with a pure `ToolPresentation` descriptor derived
from `ToolCall`, `resolved`, and `is_error`. It owns active/completed verbs,
detail text, and outcome. Both GPUI and any other viewport can consume the same
semantics without putting UI framework types in the protocol crate.

Command presentation is explicit:

- unresolved: `Running command`;
- resolved success: `Ran command` plus `Success`;
- resolved error: `Ran command` plus `Failed`.

Other tool kinds use active/completed verbs (`Reading`/`Read`,
`Editing`/`Edited`, `Searching`/`Searched`, and equivalent bounded labels). They
do not all receive a redundant success badge.

### 3. Auto-open only the active tool detail

The group-level `auto_open` contract stays unchanged. Inside that active group,
the default detail state opens for an unresolved command/file change and for
the last command that resolved while the assistant entry is still streaming.
Concurrent unresolved top-level tools may all remain open. A user toggle is an
explicit override and continues to win over the derived default.

When the assistant entry settles, the derived default becomes closed. The
existing fold state and analytic resize animation perform the transition; no
new component-local expansion store is introduced.

### 4. Distinguish invocation from result visually

Invocation and output remain `ToolDetail` values but render with an explicit
role:

- invocation: foreground command/input, with the Exec `$` prompt in the theme's
  amber syntax tone;
- successful result: muted foreground;
- failed result: danger-muted foreground;
- diff and stats continue using the changes component and semantic add/delete
  colors.

Cards retain the current nine-pixel radius, faint `ink` wash, hairline border,
fixed header footprint, contextual icon tile, and bounded detail heights.
Running indicators occupy the existing trailing slot so state changes do not
move layout.

### 5. Add optional execution metadata, not invented streams

Add an optional `ToolExecutionMeta` to `AgentEvent::ToolResult` and
`MessagePart::Tool`:

```rust
pub struct ToolExecutionMeta {
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
}
```

The existing `output: Option<String>` remains the canonical combined text.
Do not invent stdout/stderr separation when a provider reports only one output
string. The Loro shape gains additive `execution` data; old rows deserialize as
`None`, and old writers/readers keep their existing behavior.

Adapters preserve what they actually receive:

- Codex: `aggregatedOutput`, `exitCode`, and `durationMs` from completed
  `commandExecution` items;
- Claude: tool-result text content when it is safe and renderable; no fabricated
  exit code;
- OMP: existing output plus `details.exitCode`/duration when present;
- ACP: existing output plus structured execution fields when present;
- Cursor: existing `resolved/is_error` fallback when no metadata is reported.

Output continues through the existing bounded-summary and sidecar privacy
policy.

### 6. Persist reasoning as its own part

`ReasoningDelta` is currently discarded by `fold_event_into_parts`. Add a
`MessagePart::Reasoning` variant with stable id, text, completion state, and
optional duration. Consecutive deltas append to one reasoning part. The next
non-reasoning content event or terminal `Done` closes the current reasoning
part.

The first version does not require every harness to emit explicit reasoning
start/end frames. Where a runtime provides measured duration, carry it;
otherwise the UI shows a live elapsed clock scoped to the stable part and
settles to `Thought` without inventing a historical duration after remount.

The transcript gains a dedicated row:

- active: sparkle + one-pass shimmer + `Thinking` + live elapsed value;
- settled: `Thought` or `Thought for …` when measured duration exists;
- trace open while active, closed when settled unless the user toggled it;
- reasoning text rendered through the existing Markdown tree and muted trace
  rail, not as plain unstructured text.

### 7. Preserve performance and compatibility

- Streaming row identity and versions remain stable.
- New execution/reasoning fields participate in fingerprints so only affected
  rows splice.
- No syntax highlighting work is added per tool delta.
- Spinner cells animate inside fixed slots and respect GPUI reduced motion.
- Old Loro documents and sessions without new fields render unchanged.
- Reasoning is bounded by the same doc/rebuild policies as other message parts;
  it must not leak into tool summaries or final-answer extraction.

## Implementation boundaries

### Protocol and shared presentation

- `crates/proto/src/agent.rs`: additive execution metadata and event field.
- `crates/proto/src/view.rs`: lifecycle presentation descriptor.

### Runtime normalization

- `crates/harness/src/{claude,codex,omp,acp,cursor}`: preserve only reported
  output/metadata, with existing fallbacks.

### Durable fold

- `crates/doc/src/parts.rs`: execution metadata and reasoning fold.
- `crates/doc/src/schema.rs`: additive Loro mirror and salvage behavior.
- `crates/engine/src/sessions.rs`: render/privacy policy and reasoning closure.

### GPUI transcript

- `crates/ui/src/transcript.rs`: status trail, derived detail defaults,
  invocation/result tones, reasoning rows, analytic heights, fingerprints, and
  focused tests.
- Existing `crates/ui/src/loaders.rs`, Markdown renderer, changes renderer,
  theme, motion, and tool icon resolver are reused.

## Testing

Implementation follows TDD and frequent local commits:

1. Shared presentation tests for active/completed/error labels.
2. Protocol serde tests proving optional execution metadata is additive.
3. Harness fixtures proving Codex/Claude/OMP output and execution fields are
   preserved without changing runtimes that lack them.
4. Doc fold/schema tests for old/new Tool parts, reasoning append/close, salvage,
   privacy stripping, and continuation joins.
5. Transcript tests for ordinary-tool spinner state, command success/failure,
   active detail auto-open, settled collapse, manual override, role-specific
   tones by pure presentation inputs, and reasoning row lifecycle.
6. Focused crate tests, `cargo check --workspace`, build, and one bounded live
   visual comparison against Orchestrator.dev.

## Non-goals

- No new runtime or provider.
- No change to Pi or any existing runtime selection.
- No universal tool-output delta protocol.
- No fabricated stdout/stderr split.
- No redesign of composer, user message cards, context gauge, sidebar, Workers,
  or Markdown typography.
- No push, release, or promotion to `main` unless requested separately.
