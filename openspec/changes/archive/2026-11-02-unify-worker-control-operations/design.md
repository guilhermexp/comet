# Design

## Decisions

### D1 — Shared operations, separate policy
`unpeel_core::session_control` exposes the typed operations: `read_target`/`live_target` (path-checked manifest, any state or running), `hosted_sessions(SessionFilter)`, `read_output(id, OutputCursor)`, `read_transcript(id, TranscriptProfile)`, `page_text`, `parse_wait_target`/`judge_wait`, `encode_keys`, `strip_ansi`, `send_text` and `send_keys`. Policy stays in each adapter and runs first: the peer's `require_session` (reach grant, self, exited, same group, directional approval) and the controller's authority marker and parent/episode checks. The shared layer cannot launch, stop, archive, restart or bind parents, and it never decides who may call it.

### D2 — Authority and profile are values, not process guesses
`InputAuthority::Peer` and `InputAuthority::Controller { task_episode }`. `send_text` remains the one place that calls `session_input::deliver_sanitized_text[_with_task_receipt]`; only the controller variant can carry a task receipt. Read profiles and wait vocabularies are values the same way. The variant documents and routes the call; it is not a security boundary in itself (same OS user), the adapters' checks are.

### D3 — Superset key vocabulary
Both `ctrl+x` and `ctrl-x`, named keys from both sides (`enter`, `tab`, `shift+tab`, `escape`, arrows, `home`, `end`, `page_up`, `page_down`, `delete`, `backspace`, `space`), and `text:<literal>`. Bounds: at most 64 keys per call and 16 KiB per `text:` entry. Unknown keys are rejected before any write.

### D4 — One operation per verb, two read profiles, two wait vocabularies
- **List/inspect.** Both enumerate through `hosted_sessions`: the peer with `Running` and its reach filter, the controller bootstrap with `All` and its display order. Each projects its own fields (peer relation/can_control; controller Workers registry rows). Target loads go through `read_target`.
- **Output.** `read_output` returns bytes, the start offset, next offset, truncation and exit state. The peer reads `Tail` (low-context, bounded) and renders text; the controller's `/mobile/output` reads `From { offset }` and returns its base64 page. Controller text cleanup (`worker_output_text`) stays controller-owned.
- **Transcript.** `TranscriptProfile::Compact` applies the Settings range with the peer cap and tools override and renders the peer header; `Whole` reads the Settings range (0 = whole) for the controller, which pages it with `page_text` at 96 KiB (`start_offset`/`next_offset`/`previous_offset`).
- **Wait.** `WaitVocabulary::Peer` accepts `starting|working|blocked|done|idle|exited|unknown`, treats `done` and `idle` as one settled state and never sees task evidence. `WaitVocabulary::Controller` accepts `completed|running|exited|starting|working|blocked|done|idle`; `completed` matches only current-episode evidence (never `idle` or `exited`), a blocked Worker yields needs-input for a terminal wait, an interrupted episode ends a completion wait, and a lifecycle wait reports a fresh completion. Each adapter keeps its own polling, activity derivation, deadlines, cancellation, parent-message probe and answer shape.

## Verification
Unit (shared): vocabulary/bounds, hostile ids refused by every read, page walk on char boundaries, `completed`≠`idle`/`exited`, `done`≈`idle` for peers only, needs-input/interrupted/episode verdicts. Peer adapter: hostile targets refused before any shared read, `completed` refused (no task authority), existing self/exited/cross-group-under-Deny write refusals and low-context transcript defaults. Controller adapter: `/mobile/output` refuses hostile ids, the controller wait vocabulary is the shared one and idle/exited without evidence times out while episode evidence matches; existing paging, parent-message, interruption and send/receipt tests.
