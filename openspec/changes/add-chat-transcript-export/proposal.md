# Change: Chat Transcript Export (download/copy as Markdown, JSON, Text)

## Why

A Chat's record is trapped in the app. There is no way to hand a conversation
to someone, archive it outside comet, or feed it back to another agent — the
session menu offers only Rename, Archive and Delete. The reference
implementation (orchestrator.dev's "Export workspace" submenu, read from its
own source at `~/actions-runner/_work/orchestrator.dev/orchestrator.dev`)
solves exactly this with six actions over three formats, and comet already
holds every input it needs.

The one thing comet has that looks like an answer — the Run Journal — is the
wrong source, and saying so is half the point of this change (see
[ADR 0002](../../../docs/adr/0002-chat-transcript-export-reads-the-transcript-not-the-journal.md)).

## Decisions

- **D-01:** The exported unit is a **Chat**, never a Session. `Session` already
  names the per-device run state (`Idle | Working | AwaitingInput | Errored`)
  in `zeron_proto::entities`, so "export the session" would name the wrong
  thing. Menu label, filename, JSON fields and spec language all say Chat.
  Recorded in `CONTEXT.md`.
- **D-02:** The source is the **Chat Transcript** — the same entries the
  transcript view renders. The Run Journal is rejected as a source: it keeps
  the tool payloads `sanitize_tool_call` strips before anything is displayed or
  synced — file contents on a write, an edit's before/after strings, a fetch's
  prompt, MCP and unknown-tool inputs — so exporting it would leak, in a file
  meant to be pasted elsewhere, exactly what the transcript exists to withhold.
  A command, a read path and a search pattern survive the filter and DO appear
  in an export. Sidecar blobs
  behind `output_ref` are NOT resolved either: a fetch per tool chip buys size
  and latency without buying completeness, since the stripped inputs are gone
  regardless. ADR 0002.
- **D-03:** Scope is the Chat alone. A subagent doc is not exported; its chip
  renders as a one-line tool entry like any other. The Artifact index is the
  only place that records a subagent ran, which is why D-06 keeps it.
- **D-04:** One pure renderer produces all three formats from one pass over
  the entries, so `markdown`, `text` and `json` can never disagree about what
  the Chat contains. Tool parts render as one line each, shaped per tool
  (Bash → fenced command, Write/Edit → `> Modified: path`, Read →
  `> Read: path`, else `> *Used X tool*`), mirroring the reference. Verbose
  tool output never enters any format.
- **D-05:** No new RPC. Exporting the SELECTED Chat reads `AppState::transcript`,
  already in memory. Exporting any other row opens a transient
  `WatchDocMessages`, takes the first reset frame, and drops the subscription —
  reusing the path that already serves every doc, local or host-owned.
- **D-06:** The export opens with an Artifact index built only from what the
  transcript already carries: file writes (`diff_stats` / `file_preview`),
  heavy outputs (`output_bytes`), and subagents (`subagent_ref`). Addresses are
  ordinal (`message N, part M`). The reference's RFC 6901 JSON Pointers are
  dropped: with no sub-chat level, the pointer costs more than the ordinal it
  replaces.
- **D-07:** Download writes into the user's Downloads directory without a save
  dialog and confirms through the sidebar notice strip, matching the
  reference's one-click gesture. Copy writes to the clipboard and confirms the
  same way. Failures use the same strip — nothing about an export is silent.
  The desktop banner (`notify::post`) is NOT used: it is the background-alert
  channel and is suppressed while the app is frontmost, which is precisely when
  an export happens.
- **D-08:** The six actions sit in a flat `EXPORT` section of the existing chat
  context menu, between Archive and Delete. comet has no submenu primitive and
  this change does not add one; `menu_heading` + `menu_separator` are already
  the repo's idiom, and every action lands in one click instead of two.

## Non-goals

- Exporting more than one Chat at a time.
- Any export that contains a tool's raw input or full output (ADR 0002).
- A submenu primitive for gpui popovers.
- Re-importing an export back into comet.
