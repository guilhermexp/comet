# Change: Add CLI Worker Index to Chat Transcript Export

## Why

When a Chat triggers background CLI workers (via `zeron-workers-unpeel` / `WorkersModel`), the workers perform tasks and mutations in their own sessions. While the chat transcript export currently lists subagents, file writes, and heavy outputs under `## Artifacts`, it does not list the CLI workers dispatched by the session. Anyone receiving the exported journal has no way to reference or reach the worker sessions that did the work.

This change does not violate [ADR 0002](../../../docs/adr/0002-chat-transcript-export-reads-the-transcript-not-the-journal.md): worker session metadata is read from the in-memory **workers store** (`WorkersModel::sessions_for_parent_chat` + `project_chat_workers`), not from the Run Journal and not from unsanitized MCP input payloads.

## Decisions

- **D-01:** Add `ArtifactKind::Worker` (`markdown_label: "Worker"`, `text_label: "worker"`, JSON: `"worker"`).
- **D-02:** Worker artifacts contain the worker session ID (for reachability) and worker title, formatted consistently with existing artifacts:
  - Markdown: `- **Worker**` followed by the title and session ID in inline-code fences; each field is normalized to one line and its fence is longer than any backtick run in the value.
  - Text: `- worker {title} {session_id}`, with each field normalized to one line.
  - JSON: `{"kind": "worker", "tool": "{title}", "sessionId": "{session_id}"}`
  JSON preserves the original values.
  Worker artifacts omit `messageIx` and `partIx` (serialized without them in JSON via `skip_serializing_if`).
- **D-03:** Synchronous capture in `Shell::export_chat`: workers are resolved synchronously before `cx.spawn` via `self.workers_model.read(cx).sessions_for_parent_chat(&chat_id)` and projected via `project_chat_workers`. A worker-join error degrades the document to an empty worker list without blocking delivery, but a successful delivery is reported as `Incomplete` with the join reason instead of clean success.
- **D-04:** Markdown header worker count: when at least one worker is associated with the chat, `render_markdown` emits `\n**Workers:** {count}` in the header block alongside `**Project:**` and `**Branch:**`. When zero workers exist, the header is byte-identical to the baseline.
- **D-05:** Zero regression: exports with zero workers produce byte-identical output across Markdown, Text, and JSON formats compared to the prior implementation.
- **D-06:** Deterministic ordering: worker ordering in artifacts matches the stable sorting already provided by `project_chat_workers` (active first, newer first).

## Non-goals

- Modifying workers storage (`crates/workers-unpeel/`, `parent_notifications.rs`, `controller_mcp.rs`).
- Adding new ticket fields, preset IDs, or database migrations.
- Archiving any OpenSpec change.
