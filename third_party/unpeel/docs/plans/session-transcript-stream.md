# Terminal Session Conversation Feed

> **Status (2026-08-10):** Directional / not started. The first product slice
> is decided: a **read-only desktop companion pane** beside the live terminal.
> The terminal remains the interactive and authoritative surface. This plan
> does not add a phone chat view, a composer, or native approval controls.

## Goal

Render a running terminal session as a live, structured conversation: user and
assistant messages, collapsed reasoning, and concise tool/file activity. The
feed is derived from the provider's own conversation store and displayed next
to the retained Ghostty surface.

This is a semantic mirror of a terminal session, not a new session kind and not
a replacement terminal. Unsupported providers, setup screens, raw shells, and
anything the transcript adapter cannot represent continue to use the terminal
alone.

## Decisions

- **Surface:** macOS desktop, as a right-hand companion pane in
  `TerminalArea.swift`.
- **V1 control model:** read-only. Input, menus, questions, and approvals stay
  in the terminal.
- **Source:** provider-owned transcript files through `unpeel-core`; never
  infer messages by scraping PTY output.
- **Lifetime:** observe only the selected session while its pane is open. Do
  not run a watcher for every hosted session.
- **Fallback:** if a transcript is unavailable, stale, or unsupported, explain
  that in the pane and leave the terminal fully usable.
- **Presentation state:** pane visibility and width are local UI preferences,
  not shared Host/session state and not state-bus events.

## Relationship to the other plans

This plan has a deliberately narrow role:

- `dual-mode-sessions.md` is about **structured UI sessions** that launch a
  provider's headless protocol instead of a TUI. Those sessions can eventually
  reuse the conversation renderer, but they have a first-party event stream and
  native approval path. This plan only mirrors existing terminal sessions.
- `chat-sessions.md` is the Room-backed `unpeel-chat` app. A channel is not a
  Session; only an optional owner-authorized agent participant may reuse this
  feed reader. It is not the desktop companion pane.
- `unpeel-plugins.md` describes generic artifact/App panels. The companion
  pane should follow the same layout direction, but V1 must not wait for or
  invent the Horizon B plugin protocol.
- `host-controller-transports.md` owns the canonical remote Host contract. A
  later remote conversation stream goes through that shared router; it must not
  become another pair of independently maintained Swift and TUI routes.

## What Touchgrass proves

`touchgrass/packages/cli/src/cli/run.ts` has two useful implementation lessons:

1. `watchSessionFile` tails only appended bytes, buffers an incomplete trailing
   JSONL line, resets after truncation, uses `fs.watch`, and backs it with a
   two-second poll because filesystem events can be missed.
2. Its parser retains cross-record state: tool-call id to tool name/input maps
   and provider-specific buffers such as Kimi's streamed text parts.

Touchgrass does **not** provide a generic token stream. It emits only after a
complete provider record is available, and providers differ in how often they
write those records. `AskUserQuestion` content is read from Claude's transcript
tool-use block; it is not supplied by the lifecycle hook.

The watcher/reliability pattern is worth adopting. The Touchgrass normalized
shape and Telegram batching are not: Unpeel already has broader typed adapters
and needs stable replay/history semantics for a native UI.

## Current Unpeel baseline

Reuse:

- `crates/unpeel-core/src/transcripts/mod.rs` resolves and normalizes nine
  file-backed providers into `TranscriptEntry` / `TranscriptBlock`.
- Snapshot, history, and incremental JSONL reads already exist, including a
  bounded byte reader and truncation detection.
- `unpeel-host __transcript__ snapshot|stream|history|markdown` exposes those
  one-shot reads.
- `UnpeelShared` contains transcript DTOs, and the iOS dev bridge maps the
  one-shot commands into them.

Do not mistake that baseline for a production live feed:

- `read_transcript_stream` is one-shot and JSONL-only; Cline and some Gemini
  conversations are whole JSON documents.
- Every stream call creates fresh parser maps. A tool result in a later chunk
  can lose the tool name/input established by an earlier chunk, and
  provider-specific fragment buffers do not survive calls.
- Most entries have no stable id or sequence. The dev bridge invents ids from
  `read offset + array index`, which are not stable across snapshot, history,
  and reconnect paths.
- The existing `{next_offset, partial}` cursor advances past an unfinished
  line. It works for a polling client that faithfully returns the partial
  string, but it is a poor public replay contract and the partial line itself
  is not renderable assistant text.
- There is no production `/mobile` snapshot/stream/history route or desktop
  transcript view today. `/mobile/transcript-markdown` is only the Copy
  transcript path.
- Provider lifecycle hooks carry activity, tool name, prompt text where
  available, and provider transcript identity. They do not carry a complete,
  answerable provider prompt.
- `/mobile/approvals/answer` answers Unpeel **MCP** write/browser/computer
  approvals only. It is not a provider-terminal approval API.

## Target architecture

```text
provider JSONL / JSON document
            │
            ▼
unpeel-core conversation feed
  resolver + stateful adapter + stable cursor
            │
     active file observation
  filesystem event + stat fallback
            │
      ┌─────┴────────────────────┐
      ▼                          ▼
local `__transcript__ follow`    canonical Host stream (later)
      │                          │
      ▼                          ▼
desktop ConversationPane        remote desktop/phone consumers
      │
      └──── rendered beside the live Ghostty terminal
```

The feed belongs in `unpeel-core`. The desktop app needs an executable bridge
because it does not link the Rust crate directly: a long-lived
`unpeel-host __transcript__ follow <session-id>` child exists only while the
pane is open and emits framed NDJSON on stdout. It is an ephemeral reader, not
a daemon or hosted session.

When remote Host scope exists, `SelectedHostScope` chooses the remote stream
instead. It must never launch the local helper against local files while a
remote Host is selected.

## Feed contract

Add a V2 feed contract rather than treating the current offset/partial DTO as
the final wire protocol.

### Cursor and source identity

Use the byte offset immediately after the last **complete** record. Leave an
unfinished trailing record unread until it is complete; do not make clients
round-trip provider JSON in a `partial` field.

A cursor identifies:

- an opaque source generation (provider conversation/path identity plus file
  replacement generation); and
- the committed complete-record byte offset.

Do not expose absolute provider transcript paths to remote controllers.

The Host emits a reset when the resolved transcript changes, the file shrinks
or is replaced, a JSON document is rewritten, or a cursor is too old to replay
within the byte cap. A reset contains a fresh bounded snapshot and an explicit
reason; the client never silently bridges a gap.

### Stable item identity

Use a provider id when one is genuinely stable. Otherwise derive an id from
`source generation + record start offset + entry ordinal within that record`.
Snapshot, history, follow, and reconnect must produce the same id for the same
item.

The feed reducer may upsert an existing item when a provider writes fragments
that belong to one assistant message. Add an optional turn/group id where the
provider exposes one; otherwise group consecutive activity between user turns.
Do not claim token-level streaming for providers whose files only contain
completed messages.

### Frames

Keep the vocabulary small and versioned:

- `hello`: protocol version, session id, provider, capabilities, and source
  generation;
- `snapshot`: bounded recent items plus history cursor;
- `upsert`: one or more new/changed items plus the next committed cursor;
- `reset`: fresh snapshot plus `source_changed`, `truncated`, `replaced`, or
  `cursor_expired` reason;
- `unavailable`: supported-but-not-resolved vs unsupported/fallback reason;
- `end`: session exited or the subscriber closed.

Heartbeats are transport details, not conversation items.

## Parsing requirements

Refactor the existing adapters behind an incremental reducer that retains
provider state for the life of a subscription:

- tool-call id → name/input correlation;
- active turn/message grouping;
- provider fragment buffers;
- stable ids and record offsets;
- bounded deduplication for provider files that repeat equivalent records.

Initial snapshot and live tail must run through the same reducer so its state at
the tail is correct. History paging remains a read-only older window and must
not mutate the live reducer.

For append-only JSONL, emit committed record deltas. For replace-in-place JSON
documents, watch mtime/size and emit a bounded snapshot reset after a rewrite;
do not pretend the file supports byte-tail semantics.

Keep the existing snapshot/history/stream/markdown commands compatible for MCP,
Copy transcript, and dev tooling. The feed can share their adapters while
providing the stronger identity/replay contract.

## Observation and backpressure

- Watch the resolved source only while there is a subscriber.
- Use a filesystem notification for latency and a periodic stat as the
  correctness fallback. Hooks may accelerate transcript resolution because
  they capture provider ids/paths, but feed correctness must not depend on a
  hook firing.
- If no source resolves yet, retry with bounded backoff; fresh sessions often
  create their transcript only after the first prompt.
- Coalesce bursts before parsing, then drain until file size is caught up so an
  event arriving during a read cannot be lost.
- Bound reads, parsed item size, queued frames, and UI memory. If a slow client
  falls behind the replay limit, send `reset(cursor_expired)` rather than grow
  an unbounded queue.
- Do **not** send transcript growth through `state_bus`. That bus means shared
  state changed and listeners should re-read; it is not a high-frequency data
  stream.

## Desktop companion pane

Add a Conversation button to the selected agent session's titlebar, near the
existing gallery affordance. The button is shown only when the command has a
transcript adapter; resolution may still show a loading/unavailable state.

When open:

- the terminal stays mounted and interactive on the left;
- a draggable right pane renders a virtualized/lazy message list;
- opening, closing, and resizing the pane refits the visible Ghostty surface
  once, without recreating its cached terminal;
- switching sessions cancels the old feed process before binding the new one;
- pane width is clamped so the terminal retains a usable minimum width;
- pane visibility and width persist as desktop presentation preferences.

Presentation:

- user and assistant messages are primary;
- reasoning is collapsed and visually quiet;
- tool calls/results are compact summary rows;
- provider tool activity uses the same compact, domain-neutral summary rows;
  there is no privileged file-change list, path/count dashboard, diff, file
  tree, or editor control;
- `AskUserQuestion` may render as a read-only question/tool card when the
  transcript contains it, with a clear “Answer in terminal” affordance;
- while the user is at the bottom, follow new items; after they scroll away,
  preserve position and show a new-items button;
- history loads upward through the existing history reader;
- reset/gap/unavailable states are visible and recoverable, never silent.

Implement a small main-actor `ConversationFeedStore` that reduces
snapshot/upsert/reset frames and owns scroll/follow state. Decode the helper's
stdout off the main actor. A helper crash changes the pane to a retryable error
without touching the hosted PTY.

The pane is experimental for the first release. It is not shown for shells or
unsupported providers, and closing it returns the current desktop layout with
no session-side mutation.

## Phases

### Phase 0 — Contract and fixtures

1. Add representative fixtures for every currently file-backed provider,
   including cross-record tool call/result pairs, fragments, malformed lines,
   truncation, replacement, and transcript-path changes.
2. Define the cursor/generation/id rules and V2 frames in Rust tests first.
3. Record an acceptance matrix per provider: JSONL append, JSON document
   replace, stable provider ids, tool correlation, fragment behavior, and live
   latency expected from that provider's storage.

Exit: the plan no longer relies on assumptions such as “partial means token
stream” or “every adapter is append-only.”

### Phase 1 — Stateful core feed and local follow bridge

1. Add the incremental reducer and stable record-offset identities under the
   transcript module.
2. Add active-only file observation, stat fallback, source re-resolution,
   reset semantics, and bounded backpressure.
3. Add `unpeel-host __transcript__ follow <session-id>` and preserve the
   current one-shot CLI contract.
4. Prove snapshot → follow → reconnect → history without duplicates or gaps.

Exit: a fixture writer and a real Claude/Codex session can be followed from the
CLI with stable ids and explicit resets.

### Phase 2 — Read-only desktop pane

1. Add the feature flag, titlebar affordance, split layout, and local
   presentation preferences.
2. Add the Swift feed process/service, reducer, history paging, and session
   switch/cancellation behavior.
3. Render messages, collapsed reasoning, tool summaries, and file-change
   summaries under the product guardrails.
4. Add loading, unavailable, source-reset, process-failure, empty, and exited
   states.

Exit: a user can work in the live terminal while the companion pane paints
recent history and receives every newly committed provider record without
flicker or duplicates.

### Phase 3 — Provider hardening

1. Run the same follow contract against all file-backed adapters.
2. Handle JSON-document sources with snapshot resets and fix provider-specific
   grouping/tool correlation exposed by the matrix.
3. Add resource limits and large-transcript performance tests.
4. Update `docs/agents/transcripts.md` and
   `docs/feature/remote-transcript-api.md` to match the shipped contract and
   actual provider status.

Exit: provider differences change fidelity, not feed correctness; unsupported
or unresolved sessions degrade honestly to the terminal.

### Phase 4 — Canonical remote Host stream

1. Put conversation snapshot/history/follow behind
   `unpeel-core::controller_api` from `host-controller-transports.md`.
2. Carry the same frames over the pinned Host transport with transcript
   capability/version advertisement.
3. Run the Host conformance suite against native-app and TUI/headless Host
   adapters; do not add parallel route logic independently to
   `MobileRemoteServer.swift` and `mobile.rs`.
4. Make the desktop `RemoteSessionBackend` use that stream when a remote Host
   is selected.

This phase enables the same desktop pane for remote sessions. It does not by
itself add a phone chat screen; the phone remains terminal-first.

### Later — interaction and structured sessions

A composer for terminal sessions can eventually route through the existing
host input/deliver-text choke point, but it is a separate product and safety
phase. Provider approval controls must not be fabricated from lifecycle hooks
or wired to the unrelated MCP approval endpoint.

Native prompts, cancellable tool calls, and first-class partial deltas belong
to `dual-mode-sessions.md`, where Claude stream-JSON/Codex app-server provide
structured control. When that plan reaches its normalized event phase, reuse
the renderer and feed reducer rather than create a second chat component.

## Tests and acceptance

Rust:

- complete-record cursor behavior across partial writes;
- stable ids across snapshot, history, follow, and reconnect;
- cross-chunk tool correlation and provider fragment coalescing;
- unresolved source appearing later; path switch; truncate; replace; rotate;
- JSON-document rewrite reset;
- missed filesystem event recovered by polling;
- malformed/oversized records, byte caps, slow-consumer reset, clean cancel;
- existing transcript and Markdown tests remain compatible.

Swift:

- frame decoding and reducer idempotency;
- snapshot/upsert/reset/unavailable/end state transitions;
- session switch and pane-close cancellation;
- history prepend without scroll jump;
- follow-bottom vs paused-scroll behavior;
- helper failure/retry without terminal teardown;
- split resizing keeps the retained Ghostty pane alive.

Live smoke:

- Claude and Codex turns containing assistant text, tools, and a question;
- no duplicate messages after closing/reopening the pane or restarting the app;
- a complete provider record appears promptly while the terminal continues to
  accept input;
- an unsupported shell stays terminal-only;
- file activity receives no code-specific viewer, browser, or dashboard.

## Non-goals for V1

- replacing the terminal on desktop or phone;
- sending prompts from the companion pane;
- answering provider or MCP approvals from transcript cards;
- parsing ANSI/PTY output into semantic messages;
- promising token-level deltas when the provider store does not write them;
- background indexing/watchers for every session;
- a new notification channel, state-bus event type, daemon, or remote protocol;
- diffs, file trees, editor panes, or any other IDE chrome.
