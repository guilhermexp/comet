# Design

## Reference source

The reference is orchestrator.dev's own source tree, present on this machine at
`~/actions-runner/_work/orchestrator.dev/orchestrator.dev` — not a bundle
reconstruction. The parts that matter:

| Reference | What it does | comet equivalent |
|---|---|---|
| `src/renderer/features/sidebar/agent-chat-item.tsx` (~line 358) | `ContextMenuSub` "Export workspace" with 6 items | flat `EXPORT` section in `shell.rs`'s `chat_menu` (D-08) |
| `src/renderer/features/agents/lib/export-chat.ts` | `exportChat` (Blob + `<a download>`) and `copyChat` (`clipboard.writeText`); both call one query | two thin call sites over one renderer (D-04) |
| `src/main/lib/trpc/routers/chats.ts:3125` `exportChat` | reads chat + project + all sub-chats, branches per format | `chat_export.rs`, single Chat (D-03) |
| `renderSubChatMarkdownSection` (~line 673) | per-tool one-liners; verbose payloads omitted | same shapes, driven by `ToolCall` (D-04) |
| `buildArtifactIndex` / `renderArtifactIndexMarkdown` (~line 882) | indexes file writes, subagent results, >16 KiB outputs, with RFC 6901 pointers | same three kinds from doc fields, ordinal addresses (D-06) |
| `sanitizeFilename` (~line 3208) | strips invalid chars, collapses `_`, caps at 100 | ported as-is |

## Shape

```
entries: &[SessionMessageEntry]        ← Chat Transcript (D-02)
        │
        ▼
   ExportDoc { chat meta, artifacts: Vec<Artifact>, messages }   ← one pass
        │
   ┌────┼────┐
   ▼    ▼    ▼
  md   txt  json                        ← three renderers, one truth (D-04)
```

`ExportDoc` is the seam that makes D-04 enforceable: the formats are functions
of it, so a format cannot quietly learn something the others do not know. It is
also what makes the whole of F1 unit-testable with no gpui in play.

## Where the entries come from (D-05)

```
selected chat?  ── yes ──▶ AppState::transcript (already in memory)
      │
      no
      ▼
 WatchDocMessages(chat_id) ──▶ first reset frame ──▶ drop subscription
```

The transient watch is deliberate: `WatchDocMessages` already resolves a doc
whether it lives on this device or a host, and its first frame is a full reset.
A dedicated one-shot RPC would duplicate that resolution for one caller.

## Tool one-liners

Driven by `zeron_proto::ToolCall`, whose variants already carry the fields the
reference reaches for (`command`, `path`). `zeron_proto::view::tool_chip_content`
is the existing shared "tool as one line" convention (transcript + terminal);
the export reuses its label and adds the reference's per-tool markdown shape
around it rather than inventing a second vocabulary for the same thing.

## What an export cannot say

A tool part whose input was stripped by `render_parts` exports as
`> *Used Bash tool*` with no command. This is ADR 0002 working as intended, not
a gap to close later. The Artifact index is what keeps such a run legible: it
still records the files that changed and the subagents that ran.
